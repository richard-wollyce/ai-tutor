//! Contabilidade de tokens e controle de orçamento da API (Metering & Budgeting).
//!
//! Registra cada chamada de LLM em SQLite local (`usage_ledger.db`), calculando
//! custos estimados e aplicando limites de teto mensais para proteger a margem da assinatura.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};

/// Teto padrão do plano: 2.000.000 de tokens por mês (suficiente para ~150 sessões densas).
pub const DEFAULT_MONTHLY_TOKEN_CAP: u64 = 2_000_000;
pub const WARNING_RATIO: f64 = 0.80; // 80%

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_prompt_tokens: u64,
    pub total_completion_tokens: u64,
    pub total_tokens: u64,
    pub total_cost_usd: f64,
    pub monthly_token_cap: u64,
    pub percentage_used: f64,
    pub status: BudgetStatus,
    pub total_calls: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetStatus {
    Ok,
    Warning, // >= 80% do teto
    Capped,  // >= 100% do teto
}

pub struct MeteringStore {
    db_path: PathBuf,
}

impl MeteringStore {
    pub fn open(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).ok();
        let db_path = data_dir.join("usage_ledger.db");
        let store = Self { db_path };
        store.init_schema()?;
        Ok(store)
    }

    fn conn(&self) -> Result<Connection> {
        Connection::open(&self.db_path)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS usage_records (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL,
                operation TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                prompt_tokens INTEGER NOT NULL,
                completion_tokens INTEGER NOT NULL,
                total_tokens INTEGER NOT NULL,
                estimated_cost_usd REAL NOT NULL
            )",
            [],
        )?;
        Ok(())
    }

    /// Grava uma chamada de inferência no ledger.
    pub fn record(
        &self,
        operation: &str,
        provider: &str,
        model: &str,
        prompt_tokens: u64,
        completion_tokens: u64,
    ) -> Result<f64> {
        let conn = self.conn()?;
        let total_tokens = prompt_tokens + completion_tokens;
        let cost_usd = estimate_cost(provider, model, prompt_tokens, completion_tokens);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let timestamp = format!("{now}");

        conn.execute(
            "INSERT INTO usage_records (
                timestamp, operation, provider, model,
                prompt_tokens, completion_tokens, total_tokens, estimated_cost_usd
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                timestamp,
                operation,
                provider,
                model,
                prompt_tokens as i64,
                completion_tokens as i64,
                total_tokens as i64,
                cost_usd,
            ],
        )?;

        Ok(cost_usd)
    }

    /// Retorna o resumo consolidado de consumo e o status frente ao teto mensal.
    pub fn summary(&self, monthly_cap: Option<u64>) -> Result<UsageSummary> {
        let conn = self.conn()?;
        let cap = monthly_cap.unwrap_or(DEFAULT_MONTHLY_TOKEN_CAP);

        let mut stmt = conn.prepare(
            "SELECT 
                COALESCE(SUM(prompt_tokens), 0),
                COALESCE(SUM(completion_tokens), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(estimated_cost_usd), 0.0),
                COUNT(*)
             FROM usage_records",
        )?;

        let row = stmt.query_row([], |r| {
            let p_tokens: i64 = r.get(0)?;
            let c_tokens: i64 = r.get(1)?;
            let t_tokens: i64 = r.get(2)?;
            let cost: f64 = r.get(3)?;
            let calls: i64 = r.get(4)?;
            Ok((p_tokens as u64, c_tokens as u64, t_tokens as u64, cost, calls as u64))
        })?;

        let (total_prompt_tokens, total_completion_tokens, total_tokens, total_cost_usd, total_calls) = row;
        let percentage_used = if cap > 0 {
            (total_tokens as f64 / cap as f64) * 100.0
        } else {
            0.0
        };

        let status = if total_tokens >= cap {
            BudgetStatus::Capped
        } else if percentage_used >= (WARNING_RATIO * 100.0) {
            BudgetStatus::Warning
        } else {
            BudgetStatus::Ok
        };

        Ok(UsageSummary {
            total_prompt_tokens,
            total_completion_tokens,
            total_tokens,
            total_cost_usd,
            monthly_token_cap: cap,
            percentage_used,
            status,
            total_calls,
        })
    }
}

/// Estimativa conservadora de custo em USD por modelo.
pub fn estimate_cost(provider: &str, _model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    // Tarifas de referência por milhão de tokens (Input / Output):
    // Gemini 1.5 Flash: $0.075 / $0.30
    // Claude 3.5 Haiku: $0.80 / $4.00
    // GPT-4o-mini: $0.15 / $0.60
    let (rate_in_per_m, rate_out_per_m) = match provider {
        "gemini" => (0.075, 0.30),
        "anthropic" => (0.80, 4.00),
        "openai" => (0.15, 0.60),
        _ => (0.20, 0.80),
    };

    let cost_in = (input_tokens as f64 / 1_000_000.0) * rate_in_per_m;
    let cost_out = (output_tokens as f64 / 1_000_000.0) * rate_out_per_m;
    cost_in + cost_out
}

/// Aproxima contagem de tokens a partir de contagem de caracteres UTF-8
/// quando o provedor não reportar tokens explícitos (regra heurística: ~4 caracteres por token).
pub fn approximate_tokens(text: &str) -> u64 {
    let chars = text.chars().count();
    ((chars as f64 / 3.8).ceil() as u64).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_usage_and_budget_correctly() {
        let temp_dir = std::env::temp_dir().join(format!("wollyce_test_metering_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(&temp_dir).unwrap();
        let store = MeteringStore::open(&temp_dir).unwrap();

        let cost = store.record("chat", "gemini", "gemini-1.5-flash", 1000, 200).unwrap();
        assert!(cost > 0.0);

        let summary = store.summary(Some(1500)).unwrap();
        assert_eq!(summary.total_tokens, 1200);
        assert_eq!(summary.status, BudgetStatus::Warning); // 1200 / 1500 = 80%

        let summary_capped = store.summary(Some(1000)).unwrap();
        assert_eq!(summary_capped.status, BudgetStatus::Capped); // 1200 > 1000
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

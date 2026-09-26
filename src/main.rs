//! Ponto de entrada do Wollyce AI Tutor Desktop.

use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use wollyce::metering::MeteringStore;
use wollyce::server::{start_server, AppState};

fn main() {
    println!("============================================================");
    println!("             WOLLYCE - AI TUTOR SOCRÁTICO                  ");
    println!("       Recuperação Determinística Ulpia & Local-First       ");
    println!("============================================================");

    // 1. Configura diretório de dados local do tutor e base Ulpia
    let data_dir = PathBuf::from("data/wollyce_storage");
    let base_root = data_dir.join("base");
    let knowledge_dir = base_root.join("knowledge");

    // Inicializa a base do Ulpia com agent.txt, MAP.md, sample file e índice sincronizado
    wollyce::storage::init_wollyce_base(&base_root).expect("Falha ao inicializar base do Ulpia");

    // 2. Abre o ledger de medição de tokens (SQLite)
    let metering = MeteringStore::open(&data_dir).expect("Falha ao inicializar banco de consumo SQLite");

    // 3. Obtém credencial de IA ou seleciona IA Local Soberana
    let (provider, model, key) = resolve_provider_and_key();

    println!("• Provedor Ativo: {} ({})", provider, model);
    if provider == "local" || provider == "ollama" || provider == "llama" || provider == "llama-server" || provider.starts_with("http://") {
        let endpoint = model_call::endpoint(&provider, &model);
        println!("• Modo: SOBERANO / LOCAL-FIRST (100% Offline e local)");
        println!("• Endpoint de Inferência: {}", endpoint);
    } else {
        println!("• Chave de API: {}", if key.is_empty() { "NÃO DETECTADA (offline)" } else { "CONFIGURADA (ativa)" });
    }
    println!("• Base Ulpia: {}", base_root.display());
    println!("• Diretório de Memória: {}", knowledge_dir.display());

    let state = Arc::new(AppState {
        base_root,
        knowledge_dir,
        metering,
        provider,
        model,
        api_key: key,
        history: Mutex::new(Vec::new()),
    });

    let port = 4242;
    let url = format!("http://127.0.0.1:{port}");

    // Abre o navegador padrão no SO
    println!("• Abrindo interface desktop em {}", url);
    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/C", "start", &url]).spawn();
    }
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(&url).spawn();
    }

    // Inicia o servidor HTTP local
    if let Err(e) = start_server(state, port) {
        eprintln!("Erro fatal no servidor Wollyce: {e}");
    }
}

/// Detecta chaves de API disponíveis no ambiente ou chaveiro do SO,
/// ou adota IA Local Soberana (Ollama / llama-server) como default soberano.
fn resolve_provider_and_key() -> (String, String, String) {
    // 1. Variável explícita de ambiente para forçar provedor (ex: WOLLYCE_PROVIDER=local)
    if let Ok(forced) = std::env::var("WOLLYCE_PROVIDER") {
        let p = forced.to_lowercase();
        let model = std::env::var("WOLLYCE_MODEL")
            .or_else(|_| std::env::var("LOCAL_MODEL"))
            .unwrap_or_else(|_| "qwen2.5:3b".into());
        return (p, model, String::new());
    }

    // 2. Se LOCAL_AI_URL estiver setada, assume provedor local
    if let Ok(_local_url) = std::env::var("LOCAL_AI_URL") {
        let model = std::env::var("WOLLYCE_MODEL")
            .or_else(|_| std::env::var("LOCAL_MODEL"))
            .unwrap_or_else(|_| "qwen2.5:3b".into());
        return ("local".into(), model, String::new());
    }

    // 3. Provedores de API externa se houver chaves configuradas
    if let Some(key) = model_call::keys::get("gemini") {
        return ("gemini".into(), "gemini-2.5-flash".into(), key);
    }
    if let Some(key) = model_call::keys::get("anthropic") {
        return ("anthropic".into(), "claude-3-5-haiku-20241022".into(), key);
    }
    if let Some(key) = model_call::keys::get("openai") {
        return ("openai".into(), "gpt-4o-mini".into(), key);
    }

    // 4. Default Soberano / Local-First: sem chaves externas, opera como IA local
    let default_local_model = std::env::var("LOCAL_MODEL").unwrap_or_else(|_| "qwen2.5:3b".into());
    ("local".into(), default_local_model, String::new())
}

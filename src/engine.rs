//! Gerenciador de ciclo de vida do motor de inferência local (llama-server)
//! e perfis de modelo para Desktop e Mobile.
//!
//! Fornece:
//! 1. Inicialização e encerramento controlado do processo `llama-server`.
//! 2. Desligamento automático após 10 minutos (600 segundos) de ociosidade para poupar RAM e bateria.
//! 3. Encerramento total e coordenado de todos os processos da aplicação (Quit).
//! 4. Definição de perfis Desktop (7B) e Mobile (1.5B para Android/iOS).

use std::path::PathBuf;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// Tempo de ociosidade padrão antes de desligar o llama-server: 10 minutos.
pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    pub id: &'static str,
    pub name: &'static str,
    pub default_file: &'static str,
    pub context_tokens: usize,
    pub target: &'static str,
    pub description: &'static str,
}

pub const DESKTOP_PROFILE: ModelProfile = ModelProfile {
    id: "deepseek-r1-7b",
    name: "DeepSeek R1 Distill Qwen 7B",
    default_file: "DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf",
    context_tokens: 8192,
    target: "desktop",
    description: "Modelo de 7B de alta precisão socrática e raciocínio pedagógico explícito.",
};

pub const MOBILE_PROFILE: ModelProfile = ModelProfile {
    id: "deepseek-r1-1.5b",
    name: "DeepSeek R1 Distill Qwen 1.5B",
    default_file: "DeepSeek-R1-Distill-Qwen-1.5B-Q4_K_M.gguf",
    context_tokens: 4096,
    target: "mobile",
    description: "Modelo reduzido de 1.5B ultraleve para execução autônoma em smartphones (Android/iOS).",
};

pub fn active_profile() -> ModelProfile {
    #[cfg(any(target_os = "android", target_os = "ios"))]
    {
        return MOBILE_PROFILE;
    }

    if std::env::var("WOLLYCE_MOBILE").is_ok() {
        MOBILE_PROFILE
    } else {
        DESKTOP_PROFILE
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub port: u16,
    pub model_name: String,
    pub model_file: String,
    pub model_present: bool,
    pub binary_present: bool,
    pub target: String,
    pub idle_seconds: u64,
    pub auto_shutdown_in_seconds: u64,
}

pub struct EngineManager {
    child: Arc<Mutex<Option<Child>>>,
    last_activity: Arc<Mutex<Instant>>,
    profile: ModelProfile,
    port: u16,
    auto_shutdown_secs: u64,
    should_stop_watcher: Arc<AtomicBool>,
}

impl EngineManager {
    pub fn new(port: u16) -> Arc<Self> {
        let profile = active_profile();
        let manager = Arc::new(Self {
            child: Arc::new(Mutex::new(None)),
            last_activity: Arc::new(Mutex::new(Instant::now())),
            profile,
            port,
            auto_shutdown_secs: DEFAULT_IDLE_TIMEOUT_SECS,
            should_stop_watcher: Arc::new(AtomicBool::new(false)),
        });

        // Inicia watcher de ociosidade em background
        let child_clone = Arc::clone(&manager.child);
        let activity_clone = Arc::clone(&manager.last_activity);
        let stop_flag = Arc::clone(&manager.should_stop_watcher);
        let timeout_secs = manager.auto_shutdown_secs;

        std::thread::spawn(move || {
            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_secs(5));

                let mut child_guard = match child_clone.lock() {
                    Ok(g) => g,
                    Err(_) => continue,
                };

                if let Some(child) = child_guard.as_mut() {
                    // Verifica se o processo ainda está vivo
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            eprintln!("[Wollyce Engine] llama-server finalizou externamente com código: {status:?}");
                            *child_guard = None;
                            continue;
                        }
                        Ok(None) => {}
                        Err(e) => {
                            eprintln!("[Wollyce Engine] Erro ao consultar status do processo: {e}");
                        }
                    }

                    // Checa se ultrapassou o teto de ociosidade
                    if let Ok(last_act) = activity_clone.lock() {
                        let elapsed = last_act.elapsed().as_secs();
                        if elapsed >= timeout_secs {
                            println!("[Wollyce Engine] Desligando llama-server automaticamente após 10 minutos de ociosidade (liberando RAM/GPU).");
                            let _ = child.kill();
                            let _ = child.wait();
                            *child_guard = None;
                        }
                    }
                }
            }
        });

        manager
    }

    /// Localiza o binário do llama-server no sistema
    pub fn find_binary() -> Option<PathBuf> {
        let binary_candidates = [
            PathBuf::from("/opt/homebrew/bin/llama-server"),
            PathBuf::from("/usr/local/bin/llama-server"),
            PathBuf::from("/usr/bin/llama-server"),
            PathBuf::from("./bin/llama-server"),
        ];

        for candidate in binary_candidates {
            if candidate.is_file() {
                return Some(candidate);
            }
        }

        // Tenta checar se está disponível globalmente no PATH
        if Command::new("llama-server").arg("--version").output().is_ok() {
            return Some(PathBuf::from("llama-server"));
        }

        #[cfg(target_os = "windows")]
        if Command::new("llama-server.exe").arg("--version").output().is_ok() {
            return Some(PathBuf::from("llama-server.exe"));
        }

        None
    }

    /// Localiza o arquivo de pesos do modelo (.gguf)
    pub fn find_model(&self) -> Option<PathBuf> {
        let model_env = std::env::var("WOLLYCE_MODEL_PATH").ok().map(PathBuf::from);
        let model_candidates = [
            model_env.unwrap_or_else(|| PathBuf::from("models").join(self.profile.default_file)),
            PathBuf::from("models").join(self.profile.default_file),
            PathBuf::from(self.profile.default_file),
        ];

        model_candidates.into_iter().find(|p| p.is_file())
    }

    /// Atualiza o marcador de última atividade (reseta o temporizador de 10 min de ociosidade).
    pub fn touch(&self) {
        if let Ok(mut last) = self.last_activity.lock() {
            *last = Instant::now();
        }
    }

    /// Verifica se o processo do llama-server está em execução ativa.
    pub fn is_running(&self) -> bool {
        let mut guard = match self.child.lock() {
            Ok(g) => g,
            Err(_) => return false,
        };

        if let Some(child) = guard.as_mut() {
            match child.try_wait() {
                Ok(None) => true,
                _ => {
                    *guard = None;
                    false
                }
            }
        } else {
            false
        }
    }

    /// Retorna o PID do processo em execução, se houver.
    pub fn pid(&self) -> Option<u32> {
        let mut guard = self.child.lock().ok()?;
        if let Some(child) = guard.as_mut() {
            if child.try_wait().ok()? == None {
                return Some(child.id());
            }
        }
        None
    }

    /// Retorna o status detalhado para a interface e telemetria.
    pub fn status(&self) -> EngineStatus {
        let running = self.is_running();
        let pid = self.pid();
        let elapsed = self.last_activity.lock().map(|t| t.elapsed().as_secs()).unwrap_or(0);
        let auto_shutdown_in = if running && elapsed < self.auto_shutdown_secs {
            self.auto_shutdown_secs - elapsed
        } else {
            0
        };

        let binary_present = Self::find_binary().is_some();
        let model_present = self.find_model().is_some();

        EngineStatus {
            running,
            pid,
            port: self.port,
            model_name: self.profile.name.to_string(),
            model_file: self.profile.default_file.to_string(),
            model_present,
            binary_present,
            target: self.profile.target.to_string(),
            idle_seconds: elapsed,
            auto_shutdown_in_seconds: auto_shutdown_in,
        }
    }

    /// Inicia o processo do llama-server com aceleração de GPU Metal (ou equivalente da plataforma).
    pub fn start(&self) -> Result<(), String> {
        let mut guard = self.child.lock().map_err(|e| format!("Falha no mutex: {e}"))?;

        if let Some(child) = guard.as_mut() {
            if child.try_wait().map_err(|e| e.to_string())?.is_none() {
                return Ok(()); // Já está rodando
            }
        }

        let binary = Self::find_binary().ok_or_else(|| {
            "Binário llama-server não localizado no sistema. Instale o llama.cpp ('brew install llama.cpp') ou configure no PATH.".to_string()
        })?;

        let model_path = self.find_model().ok_or_else(|| {
            format!(
                "Arquivo de modelo '{}' não encontrado no diretório models/. Faça o download do arquivo .gguf para prosseguir.",
                self.profile.default_file
            )
        })?;

        println!("• [Wollyce Engine] Iniciando llama-server...");
        println!("  Binário: {}", binary.display());
        println!("  Modelo: {}", model_path.display());
        println!("  Porta: {}", self.port);

        let child = Command::new(binary)
            .arg("-m")
            .arg(&model_path)
            .arg("--port")
            .arg(self.port.to_string())
            .arg("-c")
            .arg(self.profile.context_tokens.to_string())
            .arg("-ngl")
            .arg("99") // Metal offload
            .arg("--host")
            .arg("127.0.0.1")
            .spawn()
            .map_err(|e| format!("Falha ao executar llama-server: {e}"))?;

        *guard = Some(child);
        self.touch();

        println!("• [Wollyce Engine] llama-server iniciado com sucesso!");
        Ok(())
    }

    /// Encerra imediatamente o processo do llama-server, liberando RAM e GPU.
    pub fn stop(&self) -> Result<(), String> {
        let mut guard = self.child.lock().map_err(|e| format!("Falha no mutex: {e}"))?;

        if let Some(mut child) = guard.take() {
            println!("• [Wollyce Engine] Finalizando llama-server (liberando memória)...");
            let _ = child.kill();
            let _ = child.wait();
            println!("• [Wollyce Engine] llama-server desligado com sucesso.");
        }

        Ok(())
    }

    /// Alterna o estado do motor (liga se desligado, desliga se ligado).
    pub fn toggle(&self) -> Result<bool, String> {
        if self.is_running() {
            self.stop()?;
            Ok(false)
        } else {
            self.start()?;
            Ok(true)
        }
    }

    /// Encerra o llama-server e finaliza todo o software Wollyce de forma coordenada.
    pub fn shutdown_all(&self) {
        println!("============================================================");
        println!("  Encerrando Wollyce AI Tutor e liberando recursos...       ");
        println!("============================================================");

        self.should_stop_watcher.store(true, Ordering::Relaxed);
        let _ = self.stop();

        std::thread::sleep(Duration::from_millis(150));
        std::process::exit(0);
    }
}

impl Drop for EngineManager {
    fn drop(&mut self) {
        self.should_stop_watcher.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.child.lock() {
            if let Some(mut child) = guard.take() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verifies_desktop_and_mobile_profiles() {
        assert_eq!(DESKTOP_PROFILE.target, "desktop");
        assert_eq!(MOBILE_PROFILE.target, "mobile");
        assert!(DESKTOP_PROFILE.name.contains("7B"));
        assert!(MOBILE_PROFILE.name.contains("1.5B"));
    }

    #[test]
    fn engine_manager_initial_state_is_clean() {
        let manager = EngineManager::new(8080);
        assert!(!manager.is_running());
        assert_eq!(manager.pid(), None);
        let status = manager.status();
        assert!(!status.running);
        assert_eq!(status.port, 8080);
    }
}

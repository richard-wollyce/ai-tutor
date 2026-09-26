//! Servidor HTTP local embutido em Rust padrão (sem dependências externas de rede).
//!
//! Roda em 127.0.0.1:4242 para fornecer a interface Desktop interativa do Wollyce.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use serde_json::json;

use crate::ingest;
use crate::metering::{BudgetStatus, MeteringStore};
use crate::tutor::TutorSession;

pub struct AppState {
    pub base_root: PathBuf,
    pub knowledge_dir: PathBuf,
    pub metering: MeteringStore,
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub history: Mutex<Vec<(String, String)>>,
    pub engine: Arc<crate::engine::EngineManager>,
}

pub fn start_server(state: Arc<AppState>, port: u16) -> Result<(), String> {
    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr).map_err(|e| format!("Falha ao ligar porta {port}: {e}"))?;
    println!(">>> Wollyce AI Tutor ativo em http://{addr}");

    for stream in listener.incoming() {
        if let Ok(mut stream) = stream {
            let state_clone = Arc::clone(&state);
            handle_connection(&mut stream, state_clone);
        }
    }

    Ok(())
}

fn handle_connection(stream: &mut TcpStream, state: Arc<AppState>) {
    let mut raw_data = Vec::new();
    let mut buffer = [0; 4096];
    let mut content_length = 0;
    let mut header_end = None;

    loop {
        let n = match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        raw_data.extend_from_slice(&buffer[..n]);

        if header_end.is_none() {
            if let Some(pos) = raw_data.windows(4).position(|w| w == b"\r\n\r\n") {
                header_end = Some(pos + 4);
                let header_str = String::from_utf8_lossy(&raw_data[..pos]);
                for line in header_str.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        if k.trim().eq_ignore_ascii_case("content-length") {
                            content_length = v.trim().parse::<usize>().unwrap_or(0);
                        }
                    }
                }
            }
        }

        if let Some(h_end) = header_end {
            if raw_data.len() >= h_end + content_length {
                break;
            }
        }
    }

    let h_end = match header_end {
        Some(pos) => pos,
        None => return,
    };

    let header_str = String::from_utf8_lossy(&raw_data[..h_end - 4]);
    let first_line = header_str.lines().next().unwrap_or_default();
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }

    let method = parts[0];
    let full_path = parts[1];
    let (route, query) = match full_path.split_once('?') {
        Some((r, q)) => (r, q),
        None => (full_path, ""),
    };
    let body_bytes = &raw_data[h_end..];
    let body = String::from_utf8_lossy(body_bytes);

    match (method, route) {
        ("GET", "/") | ("GET", "/index.html") => {
            serve_html(stream);
        }
        ("GET", "/api/status") => {
            serve_status(stream, &state);
        }
        ("GET", "/api/document") => {
            serve_document(stream, &state, query);
        }
        ("POST", "/api/ingest") => {
            handle_ingest(stream, &state, &body);
        }
        ("POST", "/api/chat") => {
            handle_chat(stream, &state, &body);
        }
        ("POST", "/api/reset") => {
            state.history.lock().unwrap().clear();
            send_json(stream, 200, &json!({ "ok": true, "message": "Session reset." }));
        }
        ("POST", "/api/engine/toggle") => {
            state.engine.touch();
            match state.engine.toggle() {
                Ok(running) => {
                    let status = state.engine.status();
                    send_json(stream, 200, &serde_json::to_value(&status).unwrap_or(json!({ "running": running })));
                }
                Err(err_msg) => {
                    send_json(stream, 400, &json!({
                        "error": err_msg,
                        "status": state.engine.status()
                    }));
                }
            }
        }
        ("GET", "/api/engine/status") => {
            let status = state.engine.status();
            send_json(stream, 200, &serde_json::to_value(&status).unwrap_or(json!({ "error": "Failed to serialize engine status" })));
        }
        ("POST", "/api/system/quit") => {
            send_json(stream, 200, &json!({ "ok": true, "message": "Encerrando Wollyce e finalizando processos..." }));
            let engine_clone = Arc::clone(&state.engine);
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                engine_clone.shutdown_all();
            });
        }
        _ => {
            send_json(stream, 404, &json!({ "error": "Not found" }));
        }
    }
}

fn serve_document(stream: &mut TcpStream, state: &AppState, query: &str) {
    let mut file_name = String::new();
    for pair in query.split('&') {
        if let Some((k, v)) = pair.split_once('=') {
            if k == "name" {
                // Decode simple URL encoded characters (e.g. %20 -> space)
                let decoded = v.replace("%20", " ").replace("+", " ");
                file_name = decoded;
                break;
            }
        }
    }

    if file_name.is_empty() || file_name.contains("..") || file_name.contains('/') || file_name.contains('\\') {
        send_json(stream, 400, &json!({ "error": "Invalid document name" }));
        return;
    }

    let target_path = state.knowledge_dir.join(&file_name);
    match std::fs::read_to_string(&target_path) {
        Ok(content) => {
            let clean = crate::security::strip_dashes(&content);
            send_json(stream, 200, &json!({
                "ok": true,
                "name": file_name,
                "content": clean
            }));
        }
        Err(e) => {
            send_json(stream, 404, &json!({ "error": format!("Document not found: {e}") }));
        }
    }
}

fn serve_status(stream: &mut TcpStream, state: &AppState) {
    let summary = state.metering.summary(None).ok();

    // Conta arquivos na base de conhecimento
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&state.knowledge_dir) {
        for entry in entries.flatten() {
            if let Some(ext) = entry.path().extension() {
                if ext == "md" {
                    files.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
        }
    }

    let is_local = state.provider == "local"
        || state.provider == "ollama"
        || state.provider == "llama"
        || state.provider == "llama-server"
        || state.provider.starts_with("http://");

    let endpoint_url = model_call::endpoint(&state.provider, &state.model);

    let payload = json!({
        "provider": state.provider,
        "model": state.model,
        "has_api_key": !state.api_key.is_empty(),
        "is_local": is_local,
        "endpoint": endpoint_url,
        "knowledge_dir": state.knowledge_dir.display().to_string(),
        "indexed_files": files,
        "files_count": files.len(),
        "metering": summary,
        "engine": state.engine.status(),
    });

    send_json(stream, 200, &payload);
}

fn handle_ingest(stream: &mut TcpStream, state: &AppState, body: &str) {
    state.engine.touch();
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            send_json(stream, 400, &json!({ "error": format!("JSON inválido: {e}") }));
            return;
        }
    };

    let path_str = parsed["path"].as_str().unwrap_or("").trim();
    let custom_title = parsed["title"].as_str().unwrap_or("").trim();
    let content_text = parsed["content"].as_str().unwrap_or("").trim();
    let pdf_base64 = parsed["pdf_base64"].as_str().unwrap_or("").trim();

    let (title, text) = if !pdf_base64.is_empty() {
        let pdf_bytes = match ingest::decode_base64(pdf_base64) {
            Ok(b) => b,
            Err(e) => {
                send_json(stream, 400, &json!({ "error": format!("Base64 decoding failed: {e}") }));
                return;
            }
        };
        match ingest::extract_pdf_from_bytes(&pdf_bytes) {
            Ok(extracted) => {
                let t = if custom_title.is_empty() { "document" } else { custom_title };
                (t.to_string(), extracted)
            }
            Err(e) => {
                send_json(stream, 400, &json!({ "error": format!("PDF text extraction failed: {e}") }));
                return;
            }
        }
    } else if !path_str.is_empty() {
        let path = Path::new(path_str);
        match ingest::extract_text(path) {
            Ok(extracted) => {
                let default_title = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("documento");
                let t = if custom_title.is_empty() { default_title } else { custom_title };
                (t.to_string(), extracted)
            }
            Err(e) => {
                send_json(stream, 400, &json!({ "error": e.to_string() }));
                return;
            }
        }
    } else if !content_text.is_empty() {
        if content_text.starts_with("%PDF-") {
            send_json(stream, 400, &json!({ "error": "PDF binary uploaded as text. Please attach the file so pdftotext extracts readable text." }));
            return;
        }
        let t = if custom_title.is_empty() { "Anotacao" } else { custom_title };
        (t.to_string(), content_text.to_string())
    } else {
        send_json(stream, 400, &json!({ "error": "Informe o arquivo (.pdf, .txt, .md) ou cole o texto." }));
        return;
    };

    // Destila para formato Ulpia
    let (markdown, tokens) = ingest::distil_to_ulpia_markdown(
        &title,
        &text,
        None,
        &state.metering,
        &state.provider,
        &state.model,
        if state.api_key.is_empty() { None } else { Some(&state.api_key) },
    );

    // Salva na base de conhecimento
    let mut slug = String::new();
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug_clean = slug.trim_end_matches('-');
    let slug_final = if slug_clean.is_empty() { "document" } else { slug_clean };
    let filename = format!("{slug_final}.md");
    let target_file = state.knowledge_dir.join(&filename);

    if let Err(e) = std::fs::write(&target_file, &markdown) {
        send_json(stream, 500, &json!({ "error": format!("Failed to save file: {e}") }));
        return;
    }

    // Sincroniza o índice do Ulpia com o novo arquivo
    let _ = crate::storage::sync_index(&state.base_root);

    send_json(stream, 200, &json!({
        "ok": true,
        "title": title,
        "filename": filename,
        "saved_to": target_file.display().to_string(),
        "tokens_spent": tokens,
        "preview": markdown.chars().take(400).collect::<String>(),
    }));
}

fn handle_chat(stream: &mut TcpStream, state: &AppState, body: &str) {
    state.engine.touch();
    let parsed: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            send_json(stream, 400, &json!({ "error": format!("JSON inválido: {e}") }));
            return;
        }
    };

    let message = parsed["message"].as_str().unwrap_or("").trim();
    if message.is_empty() {
        send_json(stream, 400, &json!({ "error": "A mensagem não pode ser vazia." }));
        return;
    }

    // Valida teto da assinatura
    if let Ok(summary) = state.metering.summary(None) {
        if summary.status == BudgetStatus::Capped {
            send_json(stream, 403, &json!({
                "error": "Teto de consumo de tokens atingido para o período. Suas buscas locais continuam funcionando, mas novas gerações de IA estão pausadas até o próximo ciclo."
            }));
            return;
        }
    }

    // Inicia stream de Server-Sent Events (SSE)
    let sse_header = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream; charset=utf-8\r\n\
        Cache-Control: no-cache\r\n\
        Connection: close\r\n\
        Access-Control-Allow-Origin: *\r\n\
        Access-Control-Allow-Headers: *\r\n\r\n";

    if stream.write_all(sse_header.as_bytes()).is_err() {
        return;
    }
    let _ = stream.flush();

    let send_event = |stream: &mut TcpStream, event_type: &str, data: &serde_json::Value| -> bool {
        let msg = format!("event: {event_type}\ndata: {}\n\n", data.to_string());
        if stream.write_all(msg.as_bytes()).is_err() {
            return false;
        }
        stream.flush().is_ok()
    };

    let tutor = TutorSession::new(
        state.base_root.clone(),
        state.provider.clone(),
        state.model.clone(),
        state.api_key.clone(),
    );

    let history_snapshot = state.history.lock().unwrap().clone();

    let mut stream_clone = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            let err_json = json!({ "type": "error", "error": format!("Stream clone error: {e}") });
            let _ = send_event(stream, "error", &err_json);
            return;
        }
    };

    let progress_cb = move |stage: &str, text: &str| {
        let payload = json!({
            "type": "status",
            "stage": stage,
            "message": text,
        });
        let msg = format!("event: status\ndata: {}\n\n", payload.to_string());
        let _ = stream_clone.write_all(msg.as_bytes());
        let _ = stream_clone.flush();
    };

    match tutor.interact_with_progress(message, &state.metering, &history_snapshot, progress_cb) {
        Ok(resp) => {
            // Atualiza histórico na memória
            let mut h = state.history.lock().unwrap();
            h.push((message.to_string(), resp.reply.clone()));

            let summary = state.metering.summary(None).ok();

            let final_payload = json!({
                "type": "done",
                "ok": true,
                "reply": resp.reply,
                "abstained": resp.abstained,
                "citations": resp.citations,
                "tokens_prompt": resp.tokens_prompt,
                "tokens_completion": resp.tokens_completion,
                "cost_usd": resp.cost_usd,
                "coverage": resp.coverage,
                "methodology": {
                    "name": resp.methodology_name,
                    "book": resp.methodology_book,
                },
                "confidence": {
                    "verdict": resp.confidence_verdict,
                    "keyword_score": resp.keyword_score,
                },
                "metering": summary,
            });

            let _ = send_event(stream, "done", &final_payload);
        }
        Err(e) => {
            let err_payload = json!({
                "type": "error",
                "error": e,
            });
            let _ = send_event(stream, "error", &err_payload);
        }
    }
}

fn send_json(stream: &mut TcpStream, status: u16, payload: &serde_json::Value) {
    let body = payload.to_string();
    let response = format!(
        "HTTP/1.1 {status} OK\r\n\
         Content-Type: application/json; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Access-Control-Allow-Origin: *\r\n\
         Access-Control-Allow-Headers: *\r\n\
         Connection: close\r\n\r\n\
         {}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn serve_html(stream: &mut TcpStream) {
    let html = include_str!("../ui/index.html");
    let response = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n\
         {}",
        html.len(),
        html
    );
    let _ = stream.write_all(response.as_bytes());
}

//! Esteira de ingestão restrita a .pdf, .txt e .md com destilação para o formato Ulpia.

use std::path::{Path, PathBuf};
use std::process::Command;
use crate::security;
use crate::metering::{MeteringStore, approximate_tokens};

#[derive(Debug)]
pub enum IngestError {
    UnsupportedExtension(String),
    FileNotFound(PathBuf),
    ExtractionFailed(String),
    Io(std::io::Error),
}

impl std::fmt::Display for IngestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IngestError::UnsupportedExtension(ext) => write!(
                f,
                "Extensão '{ext}' não suportada. O Wollyce aceita exclusivamente arquivos .pdf, .txt e .md"
            ),
            IngestError::FileNotFound(p) => write!(f, "Arquivo não encontrado: {}", p.display()),
            IngestError::ExtractionFailed(err) => write!(f, "Falha na extração do texto: {err}"),
            IngestError::Io(e) => write!(f, "Erro de E/S: {e}"),
        }
    }
}

impl std::error::Error for IngestError {}

/// Valida rigorosamente a extensão permitida.
pub fn validate_extension(path: &Path) -> Result<&'static str, IngestError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    match ext.as_str() {
        "pdf" => Ok("pdf"),
        "txt" => Ok("txt"),
        "md" => Ok("md"),
        other => Err(IngestError::UnsupportedExtension(other.to_string())),
    }
}

/// Extrai o texto limpo do arquivo respeitando seu formato.
pub fn extract_text(path: &Path) -> Result<String, IngestError> {
    if !path.is_file() {
        return Err(IngestError::FileNotFound(path.to_path_buf()));
    }

    let ext = validate_extension(path)?;
    let raw_text = match ext {
        "txt" | "md" => {
            let bytes = std::fs::read(path).map_err(IngestError::Io)?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
        "pdf" => extract_pdf_text(path)?,
        _ => unreachable!(),
    };

    let sanitized = security::sanitize_text(&raw_text);
    if sanitized.trim().is_empty() {
        return Err(IngestError::ExtractionFailed(
            "O arquivo não contém texto legível para aprendizado.".into(),
        ));
    }

    Ok(sanitized)
}

/// Extrai texto de PDF usando pdftotext com preservação de layout e UTF-8.
fn extract_pdf_text(pdf_path: &Path) -> Result<String, IngestError> {
    // Procura pdftotext no caminho padrão do Git for Windows ou no PATH do sistema
    let candidates = [
        PathBuf::from(r"C:\Program Files\Git\mingw64\bin\pdftotext.exe"),
        PathBuf::from("pdftotext.exe"),
        PathBuf::from("pdftotext"),
    ];

    let mut binary_to_use = None;
    for cand in &candidates {
        if cand.is_file() || cand.as_os_str() == "pdftotext.exe" || cand.as_os_str() == "pdftotext" {
            binary_to_use = Some(cand);
            break;
        }
    }

    let bin = binary_to_use.ok_or_else(|| {
        IngestError::ExtractionFailed("pdftotext não encontrado no sistema para processamento do PDF.".into())
    })?;

    let output = Command::new(bin)
        .arg("-layout")
        .arg("-enc")
        .arg("UTF-8")
        .arg(pdf_path)
        .arg("-") // Emite para stdout
        .output()
        .map_err(|e| IngestError::ExtractionFailed(format!("Falha ao invocar pdftotext: {e}")))?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        return Err(IngestError::ExtractionFailed(format!(
            "pdftotext retornou erro (código {:?}): {}",
            output.status.code(),
            err_msg
        )));
    }

    let raw_out = String::from_utf8_lossy(&output.stdout).into_owned();
    let paginated = clean_and_paginate_pdf_text(&raw_out);
    Ok(paginated)
}

/// Limpa e formata o texto extraido do PDF, unindo hifens de quebra de linha
/// e inserindo marcadores semanticos de pagina.
pub fn clean_and_paginate_pdf_text(raw_text: &str) -> String {
    let mut pages_out = Vec::new();
    let raw_pages: Vec<&str> = raw_text.split('\u{000C}').collect();

    for (i, page) in raw_pages.iter().enumerate() {
        let trimmed = page.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Desfaz hifenizacoes em quebras de linha (ex: "oper-\nator" -> "operator")
        let mut cleaned_page = String::new();
        let mut prev_line_hyphenated = false;

        for line in trimmed.lines() {
            let line_trimmed = line.trim_end();
            if line_trimmed.ends_with('-') && !line_trimmed.ends_with(" -") && !line_trimmed.starts_with('-') && !line_trimmed.starts_with('#') {
                if !prev_line_hyphenated && !cleaned_page.is_empty() {
                    cleaned_page.push('\n');
                }
                cleaned_page.push_str(&line_trimmed[..line_trimmed.len() - 1]);
                prev_line_hyphenated = true;
            } else {
                if !prev_line_hyphenated && !cleaned_page.is_empty() {
                    cleaned_page.push('\n');
                }
                cleaned_page.push_str(line_trimmed);
                prev_line_hyphenated = false;
            }
        }

        let page_num = i + 1;
        pages_out.push(format!("[[page: {page_num}]]\n{cleaned_page}"));
    }

    pages_out.join("\n\n")
}

/// Destila o texto ingerido em um arquivo Markdown formatado para o Ulpia
/// contendo título, cabeçalho de busca 'Search for:' e corpo do material.
pub fn distil_to_ulpia_markdown(
    title: &str,
    content: &str,
    custom_keywords: Option<&str>,
    metering: &MeteringStore,
    provider: &str,
    model: &str,
    api_key: Option<&str>,
) -> (String, u64) {
    let mut tokens_used = 0;

    // Se fornecidas chaves customizadas ou se estiver offline, usa heurística local
    let keywords = if let Some(kw) = custom_keywords {
        kw.to_string()
    } else if let Some(key) = api_key {
        // Gera chaves através de chamada ultraleve de IA
        let sample: String = content.chars().take(2000).collect();
        let prompt = format!(
            "Leia o seguinte trecho didatico do documento '{title}' e gere uma lista de 20 a 25 termos de busca e conceitos chave essenciais (em portugues e ingles), separados por virgula. E terminantemente proibido usar travoes. Retorne APENAS a lista de termos separados por virgula, sem preambulo.\n\nTrecho:\n{sample}"
        );

        let p_tokens = approximate_tokens(&prompt);
        match model_call::call(provider, model, key, &prompt) {
            Ok(keys_generated) => {
                let c_tokens = approximate_tokens(&keys_generated);
                tokens_used = p_tokens + c_tokens;
                metering
                    .record("ingest_distil", provider, model, p_tokens, c_tokens)
                    .ok();
                keys_generated.trim().replace('\n', ", ")
            }
            Err(_) => generate_local_fallback_keywords(title, content),
        }
    } else {
        generate_local_fallback_keywords(title, content)
    };

    let clean_keywords = crate::security::strip_dashes(&keywords);
    let clean_content = crate::security::strip_dashes(content);

    let markdown = format!(
        "# {title}\n\n\
         **Search for:** {clean_keywords}\n\n\
         {clean_content}\n"
    );

    (markdown, tokens_used)
}

/// Extrator de palavras-chave local determinístico (sem custos de API).
fn generate_local_fallback_keywords(title: &str, content: &str) -> String {
    let mut words: Vec<String> = title
        .split_whitespace()
        .map(|w| w.to_lowercase())
        .filter(|w| w.len() > 3)
        .collect();

    for line in content.lines().take(50) {
        for word in line.split_whitespace() {
            let clean: String = word
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
                .to_lowercase();
            if clean.len() > 4 && !words.contains(&clean) {
                words.push(clean);
            }
            if words.len() >= 25 {
                break;
            }
        }
        if words.len() >= 25 {
            break;
        }
    }

    words.join(", ")
}

/// Extrai texto de um PDF a partir de bytes crus em memoria, salvando temporariamente
/// em disco e invocando o pdftotext nativo.
pub fn extract_pdf_from_bytes(bytes: &[u8]) -> Result<String, IngestError> {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let temp_file = std::env::temp_dir().join(format!("wollyce_upload_{}_{}.pdf", std::process::id(), millis));
    std::fs::write(&temp_file, bytes)
        .map_err(|e| IngestError::ExtractionFailed(format!("Falha ao gravar PDF temporario: {e}")))?;

    let result = extract_pdf_text(&temp_file);
    let _ = std::fs::remove_file(&temp_file);
    result
}

/// Decodifica string em Base64 em bytes crus sem dependencias externas.
pub fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;

    for &b in input.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => (b - b'A') as u32,
            b'a'..=b'z' => (b - b'a' + 26) as u32,
            b'0'..=b'9' => (b - b'0' + 52) as u32,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' | b' ' => continue,
            _ => return Err(format!("Byte invalido em base64: {}", b as char)),
        };
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_supported_extensions_strictly() {
        assert_eq!(validate_extension(Path::new("trivium.pdf")).unwrap(), "pdf");
        assert_eq!(validate_extension(Path::new("quadrivium.txt")).unwrap(), "txt");
        assert_eq!(validate_extension(Path::new("logica.md")).unwrap(), "md");

        assert!(validate_extension(Path::new("virus.exe")).is_err());
        assert!(validate_extension(Path::new("documento.docx")).is_err());
        assert!(validate_extension(Path::new("pagina.html")).is_err());
    }

    #[test]
    fn generates_fallback_keywords() {
        let text = "A gramatica e a arte de falar e escrever corretamente segundo as regras.";
        let kw = generate_local_fallback_keywords("Gramatica Trivium", text);
        assert!(kw.contains("gramatica"));
        assert!(kw.contains("trivium"));
    }

    #[test]
    fn decodes_base64_cleanly() {
        let original = b"Hello, Wollyce!";
        let encoded = "SGVsbG8sIFdvbGx5Y2Uh";
        let decoded = decode_base64(encoded).unwrap();
        assert_eq!(decoded, original);
    }
}

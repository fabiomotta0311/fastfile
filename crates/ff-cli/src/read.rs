//! `ff read`: leitura de arquivos e trechos para agentes (docs/avaliacao-agentes.md, proposta #6).
//!
//! Vários arquivos e intervalos de linha por chamada (`a.rs:10-80`, `b.rs:120`, `c.rs:300-`),
//! leitura em fluxo (nunca carrega o arquivo inteiro para devolver um trecho), detecção de binário,
//! UTF-16 com BOM, texto de PDF/DOCX, esqueleto de símbolos (`outline`), arquivos de segredo
//! bloqueados e valores com cara de segredo mascarados por padrão. Somente leitura.

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;

use serde_json::json;

use crate::secrets::{is_secret_file, mask_secrets};
use crate::util::make_relative_path;

/// Linhas devolvidas por arquivo quando o pedido não diz o fim do intervalo.
pub const DEFAULT_MAX_LINES: usize = 400;
/// Linhas de contexto ao redor de `arquivo:N`.
pub const DEFAULT_AROUND: usize = 10;
/// Linhas mais longas que isto são cortadas (minificados, dados em uma linha só).
pub const MAX_LINE_CHARS: usize = 2000;
/// Tamanho máximo para extrair símbolos (`outline`) ou texto de documentos.
const MAX_PARSE_BYTES: u64 = 8 * 1024 * 1024;

/// Opções de uma leitura.
#[derive(Debug, Clone)]
pub struct ReadOptions {
    /// Especificações `caminho`, `caminho:N`, `caminho:A-B` ou `caminho:A-`.
    pub specs: Vec<String>,
    /// Máximo de linhas por arquivo quando o intervalo não tem fim explícito.
    pub max_lines: usize,
    /// Linhas de contexto para `caminho:N`.
    pub around: usize,
    /// Devolver só o esqueleto de símbolos (sem conteúdo).
    pub outline: bool,
    /// Orçamento de tokens (≈ 4 bytes) para todo o conteúdo devolvido.
    pub budget: Option<usize>,
    pub include_secrets: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            specs: Vec::new(),
            max_lines: DEFAULT_MAX_LINES,
            around: DEFAULT_AROUND,
            outline: false,
            budget: None,
            include_secrets: false,
        }
    }
}

/// Intervalo pedido para um arquivo (linhas começam em 1; `end` inclusivo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadSpec {
    pub path: String,
    pub start: usize,
    pub end: Option<usize>,
    /// Linha central de `caminho:N` (o intervalo já inclui o contexto).
    pub focus: Option<usize>,
}

/// Interpreta `caminho`, `caminho:N`, `caminho:A-B` ou `caminho:A-`.
/// O sufixo só é lido se for numérico, então `C:\x.rs` e `a:b.txt` continuam caminhos.
#[must_use]
pub fn parse_spec(spec: &str, around: usize) -> ReadSpec {
    let whole = ReadSpec {
        path: spec.to_string(),
        start: 1,
        end: None,
        focus: None,
    };
    let Some((path, range)) = spec.rsplit_once(':') else {
        return whole;
    };
    if path.is_empty() || range.is_empty() {
        return whole;
    }
    let parse = |s: &str| s.trim().parse::<usize>().ok().filter(|n| *n >= 1);
    if let Some((a, b)) = range.split_once('-') {
        let Some(start) = parse(a) else { return whole };
        if b.trim().is_empty() {
            return ReadSpec {
                path: path.to_string(),
                start,
                end: None,
                focus: None,
            };
        }
        let Some(end) = parse(b) else { return whole };
        return ReadSpec {
            path: path.to_string(),
            start: start.min(end),
            end: Some(start.max(end)),
            focus: None,
        };
    }
    match parse(range) {
        Some(n) => ReadSpec {
            path: path.to_string(),
            start: n.saturating_sub(around).max(1),
            end: Some(n + around),
            focus: Some(n),
        },
        None => whole,
    }
}

enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
}

/// Lê o arquivo em fluxo e devolve as linhas do intervalo, o total de linhas e a codificação.
fn read_range(
    path: &Path,
    start: usize,
    end: usize,
) -> std::io::Result<(Vec<String>, usize, &'static str, bool)> {
    let mut file = File::open(path)?;
    let mut head = [0u8; 8192];
    let n = file.read(&mut head)?;
    let head = &head[..n];

    let encoding = if head.starts_with(&[0xFF, 0xFE]) {
        Encoding::Utf16Le
    } else if head.starts_with(&[0xFE, 0xFF]) {
        Encoding::Utf16Be
    } else {
        Encoding::Utf8
    };
    if matches!(encoding, Encoding::Utf8) && head.contains(&0) {
        return Ok((Vec::new(), 0, "binary", true));
    }

    match encoding {
        Encoding::Utf8 => {
            let file = File::open(path)?;
            let mut reader = BufReader::with_capacity(64 * 1024, file);
            let mut lines = Vec::new();
            let mut buf = Vec::new();
            let mut lineno = 0usize;
            let mut lossy = false;
            loop {
                buf.clear();
                if reader.read_until(b'\n', &mut buf)? == 0 {
                    break;
                }
                lineno += 1;
                if lineno >= start && lineno <= end {
                    if lineno == 1 && buf.starts_with(&[0xEF, 0xBB, 0xBF]) {
                        buf.drain(..3);
                    }
                    let text = match std::str::from_utf8(&buf) {
                        Ok(t) => t.to_string(),
                        Err(_) => {
                            lossy = true;
                            String::from_utf8_lossy(&buf).into_owned()
                        }
                    };
                    lines.push(text.trim_end_matches(['\n', '\r']).to_string());
                }
            }
            Ok((lines, lineno, if lossy { "lossy" } else { "utf-8" }, false))
        }
        Encoding::Utf16Le | Encoding::Utf16Be => {
            if std::fs::metadata(path)?.len() > MAX_PARSE_BYTES {
                return Err(std::io::Error::other(
                    "arquivo UTF-16 grande demais para leitura parcial",
                ));
            }
            let bytes = std::fs::read(path)?;
            let units: Vec<u16> = bytes[2..]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| match encoding {
                    Encoding::Utf16Le => u16::from_le_bytes(*c),
                    _ => u16::from_be_bytes(*c),
                })
                .collect();
            let text = String::from_utf16_lossy(&units);
            let (lines, total) = slice_text(&text, start, end);
            let name = if matches!(encoding, Encoding::Utf16Le) {
                "utf-16le"
            } else {
                "utf-16be"
            };
            Ok((lines, total, name, false))
        }
    }
}

fn slice_text(text: &str, start: usize, end: usize) -> (Vec<String>, usize) {
    let mut total = 0;
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        total = i + 1;
        if total >= start && total <= end {
            out.push(line.to_string());
        }
    }
    (out, total)
}

pub(crate) fn truncate_line(line: String) -> (String, bool) {
    if line.chars().count() <= MAX_LINE_CHARS {
        return (line, false);
    }
    let cut: String = line.chars().take(MAX_LINE_CHARS).collect();
    (format!("{cut}…"), true)
}

fn error_item(path: &str, code: &str, message: String) -> serde_json::Value {
    json!({ "path": path, "error": { "code": code, "message": message } })
}

fn outline_item(display: &str, path: &Path, include_secrets: bool) -> serde_json::Value {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    if size > MAX_PARSE_BYTES {
        return error_item(
            display,
            "TOO_LARGE",
            format!("arquivo de {size} bytes grande demais para o esqueleto"),
        );
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => return error_item(display, "IO_ERROR", e.to_string()),
    };
    if ff_sym::SupportedLanguage::from_path(path).is_none() {
        return error_item(
            display,
            "UNSUPPORTED_LANGUAGE",
            "esqueleto disponível para rust, python, js, ts, go e cs".to_string(),
        );
    }
    let mut symbols = ff_sym::extract_symbols(path, &content);
    symbols.sort_by_key(|s| s.line);
    let items: Vec<serde_json::Value> = symbols
        .into_iter()
        .map(|s| {
            let sig = if include_secrets {
                s.signature
            } else {
                mask_secrets(&s.signature).0
            };
            json!({ "line": s.line, "kind": s.kind.as_str(), "name": s.name, "signature": sig })
        })
        .collect();
    json!({
        "path": display,
        "total_lines": content.lines().count(),
        "outline": items,
    })
}

/// Executa a leitura e devolve o envelope JSON completo (`schema` atual).
#[must_use]
pub fn execute_read(opts: &ReadOptions) -> serde_json::Value {
    let start_time = std::time::Instant::now();
    let mut results = Vec::new();
    let mut secrets_hidden = 0usize;
    let mut secrets_masked = 0usize;
    let mut lines_returned = 0usize;
    let mut truncated_any = false;
    let mut remaining_bytes = opts.budget.map(|b| b.saturating_mul(4));

    for raw in &opts.specs {
        let spec = parse_spec(raw, opts.around);
        let path = Path::new(&spec.path);
        let display = make_relative_path(&spec.path, ".");

        if !path.exists() {
            results.push(error_item(
                &display,
                "TARGET_NOT_FOUND",
                format!("alvo não encontrado: '{}'", spec.path),
            ));
            continue;
        }
        if path.is_dir() {
            results.push(error_item(
                &display,
                "IS_DIRECTORY",
                "é uma pasta; use find ou map para listar".to_string(),
            ));
            continue;
        }
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !opts.include_secrets && is_secret_file(name) {
            secrets_hidden += 1;
            results.push(error_item(
                &display,
                "SECRET_FILE",
                "arquivo de segredo; use include_secrets para ler".to_string(),
            ));
            continue;
        }

        if opts.outline {
            results.push(outline_item(&display, path, opts.include_secrets));
            continue;
        }

        let end_requested = spec.end;
        let end = end_requested.unwrap_or(spec.start + opts.max_lines.max(1) - 1);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let read = if ff_content::has_document_extractor(&ext) {
            match ff_content::extract_text_from_file(path) {
                Ok(Some(text)) => {
                    let (lines, total) = slice_text(&text, spec.start, end);
                    Ok((lines, total, "extracted", false))
                }
                Ok(None) => Ok((Vec::new(), 0, "binary", true)),
                Err(e) => Err(e),
            }
        } else {
            read_range(path, spec.start, end)
        };

        let (lines, total_lines, encoding, binary) = match read {
            Ok(r) => r,
            Err(e) => {
                results.push(error_item(&display, "IO_ERROR", e.to_string()));
                continue;
            }
        };
        if binary {
            results.push(json!({
                "path": display,
                "binary": true,
                "size": std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
            }));
            continue;
        }

        let mut content = String::new();
        let mut returned = 0usize;
        let mut cut_by_budget = false;
        let mut long_lines = 0usize;
        for line in lines {
            let (line, cut) = truncate_line(line);
            long_lines += usize::from(cut);
            let line = if opts.include_secrets {
                line
            } else {
                let (masked, n) = mask_secrets(&line);
                secrets_masked += n;
                masked
            };
            if let Some(rem) = remaining_bytes.as_mut() {
                let cost = line.len() + 1;
                if cost > *rem {
                    cut_by_budget = true;
                    break;
                }
                *rem -= cost;
            }
            content.push_str(&line);
            content.push('\n');
            returned += 1;
        }

        let last = spec.start + returned.saturating_sub(1);
        let has_more = (returned > 0 && last < total_lines) || cut_by_budget;
        let stopped_early = cut_by_budget || (end_requested.is_none() && last < total_lines);
        truncated_any |= stopped_early;
        lines_returned += returned;

        let mut item = json!({
            "path": display,
            "start_line": spec.start,
            "end_line": if returned == 0 { spec.start.saturating_sub(1) } else { last },
            "total_lines": total_lines,
            "content": content,
        });
        if let Some(f) = spec.focus {
            item["focus_line"] = json!(f);
        }
        if encoding != "utf-8" {
            item["encoding"] = json!(encoding);
        }
        if long_lines > 0 {
            item["long_lines_cut"] = json!(long_lines);
        }
        if stopped_early && has_more {
            item["truncated"] = json!(true);
            item["next"] = json!(format!("{}:{}-", display, last + 1));
        }
        results.push(item);
    }

    let mut warnings = Vec::new();
    if secrets_hidden > 0 {
        warnings.push(format!(
            "{secrets_hidden} arquivo(s) de segredo não lido(s); use --include-secrets para ler"
        ));
    }
    if secrets_masked > 0 {
        warnings.push(format!(
            "{secrets_masked} trecho(s) com cara de segredo mascarado(s); use --include-secrets para exibir"
        ));
    }
    if truncated_any {
        warnings.push("conteúdo parcial; continue pelo campo `next` de cada arquivo".to_string());
    }

    let mut env = json!({
        "schema": crate::schema::schema_id(),
        "source": "walk",
        "command": "read",
        "summary": {
            "files": opts.specs.len(),
            "lines_returned": lines_returned,
            "secrets_hidden": secrets_hidden,
            "secrets_masked": secrets_masked,
            "elapsed_ms": (start_time.elapsed().as_secs_f64() * 100_000.0).round() / 100.0,
        },
        "results": results,
    });
    if !warnings.is_empty() {
        env["warnings"] = json!(warnings);
    }
    if truncated_any {
        env["truncated"] = json!(true);
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_utf16_with_bom() {
        let path = std::env::temp_dir().join(format!("ff_read_utf16_{}.txt", std::process::id()));
        let mut bytes = vec![0xFF, 0xFE];
        for unit in "primeira\nsegunda ção\nterceira\n".encode_utf16() {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        std::fs::write(&path, bytes).unwrap();
        let (lines, total, enc, binary) = read_range(&path, 2, 2).unwrap();
        assert_eq!(lines, vec!["segunda ção".to_string()]);
        assert_eq!((total, enc, binary), (3, "utf-16le", false));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn spec_parsing() {
        let s = parse_spec("src/a.rs:10-20", 5);
        assert_eq!(
            (s.path.as_str(), s.start, s.end),
            ("src/a.rs", 10, Some(20))
        );
        let s = parse_spec("src/a.rs:20-10", 5);
        assert_eq!((s.start, s.end), (10, Some(20)));
        let s = parse_spec("src/a.rs:30", 5);
        assert_eq!((s.start, s.end, s.focus), (25, Some(35), Some(30)));
        let s = parse_spec("src/a.rs:3", 5);
        assert_eq!(s.start, 1);
        let s = parse_spec("src/a.rs:40-", 5);
        assert_eq!((s.start, s.end), (40, None));
        // Sufixo não numérico continua parte do caminho.
        let s = parse_spec(r"C:\x\y.rs", 5);
        assert_eq!((s.path.as_str(), s.start, s.end), (r"C:\x\y.rs", 1, None));
        let s = parse_spec("C:/x/y.rs:7-9", 5);
        assert_eq!((s.path.as_str(), s.start, s.end), ("C:/x/y.rs", 7, Some(9)));
    }
}

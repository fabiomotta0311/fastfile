//! `ff-preview`
//!
//! Motor de pré-visualização de arquivos para o FastFile (§4 e Issue #14).
//! Utiliza memória mapeada (`memmap2`) para abertura instantânea de arquivos gigantes (1 GB+)
//! sem carregar todo o arquivo na memória RAM, detecção de binário com hex dump,
//! realce de sintaxe via `syntect` e navegação de ocorrências de busca.

use std::fs::File;
use std::io;
use std::path::Path;

use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use syntect::easy::HighlightLines;
use syntect::highlighting::{Color, Theme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

/// Cor RGB simples para renderização na UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl From<Color> for HighlightColor {
    fn from(c: Color) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }
}

/// Trecho de texto com cor e sinalizador se coincide com o termo de busca pesquisado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyledSpan {
    pub text: String,
    pub color: HighlightColor,
    pub is_match: bool,
}

/// Uma linha renderizável com número e trechos estilizados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HighlightedLine {
    pub lineno: usize,
    pub spans: Vec<StyledSpan>,
}

/// Ocorrência encontrada do termo de busca no arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchOccurrence {
    pub line: usize,
    pub col: usize,
    pub len: usize,
}

/// Conteúdo de texto pré-visualizado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextPreview {
    pub lines: Vec<HighlightedLine>,
    pub total_lines: usize,
    pub syntax_name: String,
    pub occurrences: Vec<SearchOccurrence>,
}

/// Resumo e hex dump para arquivos binários.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryPreview {
    pub size: u64,
    pub hex_dump: Vec<String>,
    pub header_magic: String,
}

/// Resultado da pré-visualização de um arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PreviewResult {
    Text(TextPreview),
    Binary(BinaryPreview),
    Empty,
}

/// Detecta se um buffer representa conteúdo binário.
#[must_use]
pub fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(8192);
    if check_len == 0 {
        return false;
    }
    let slice = &bytes[..check_len];
    if slice.contains(&0) {
        return true;
    }
    // Mais de 30% de caracteres de controle não-espaço é indicativo de binário
    let control_chars = slice
        .iter()
        .filter(|&&b| b < 9 || (b > 13 && b < 32))
        .count();
    (control_chars * 100) / check_len > 30
}

/// Gera uma representação em formato hex dump clássico (16 bytes por linha).
#[must_use]
pub fn format_hex_dump(bytes: &[u8], max_bytes: usize) -> Vec<String> {
    let slice = &bytes[..bytes.len().min(max_bytes)];
    let mut lines = Vec::new();

    for (offset, chunk) in slice.chunks(16).enumerate() {
        let mut hex_part = String::with_capacity(48);
        let mut ascii_part = String::with_capacity(16);

        for (i, &b) in chunk.iter().enumerate() {
            if i == 8 {
                hex_part.push(' ');
            }
            hex_part.push_str(&format!("{b:02x} "));

            if b.is_ascii_graphic() || b == b' ' {
                ascii_part.push(b as char);
            } else {
                ascii_part.push('.');
            }
        }

        // Alinhamento para a última linha que pode ter menos de 16 bytes
        let spaces_needed = (16 - chunk.len()) * 3 + if chunk.len() <= 8 { 1 } else { 0 };
        for _ in 0..spaces_needed {
            hex_part.push(' ');
        }

        lines.push(format!(
            "{:08x}  {} |{}|",
            offset * 16,
            hex_part,
            ascii_part
        ));
    }

    lines
}

/// Motor de pré-visualização com conjuntos de sintaxe e tema syntect em cache.
pub struct PreviewEngine {
    pub syntax_set: SyntaxSet,
    pub theme: Theme,
}

impl Default for PreviewEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PreviewEngine {
    /// Inicializa o motor de prévia carregando gramáticas padrão do syntect.
    #[must_use]
    pub fn new() -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get("base16-ocean.dark")
            .or_else(|| theme_set.themes.get("Solarized (dark)"))
            .cloned()
            .unwrap_or_else(|| theme_set.themes.values().next().unwrap().clone());

        Self { syntax_set, theme }
    }

    /// Pré-visualiza um arquivo do disco utilizando memória mapeada (`memmap2`)
    /// e realce de sintaxe com syntect.
    pub fn preview_file<P: AsRef<Path>>(
        &self,
        path: P,
        search_term: Option<&str>,
        max_lines: usize,
    ) -> io::Result<PreviewResult> {
        let p = path.as_ref();
        let file = File::open(p)?;
        let meta = file.metadata()?;
        let file_len = meta.len();

        if file_len == 0 {
            return Ok(PreviewResult::Empty);
        }

        // Abre via memória mapeada (suporte a arquivos de 1 GB+ sem alocar toda a RAM)
        let mmap = unsafe { Mmap::map(&file)? };

        if is_binary(&mmap) {
            let hex_dump = format_hex_dump(&mmap, 512);
            let magic = mmap
                .get(..8)
                .map(|b| {
                    b.iter()
                        .map(|byte| format!("{byte:02X}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();

            return Ok(PreviewResult::Binary(BinaryPreview {
                size: file_len,
                hex_dump,
                header_magic: magic,
            }));
        }

        // Decodifica apenas os primeiros bytes necessários para a janela de linhas
        // ou a janela inteira se for menor que 500 KB
        let preview_bytes_len = mmap.len().min(1024 * 1024); // máximo 1 MB para buffer de prévia
        let text_slice = String::from_utf8_lossy(&mmap[..preview_bytes_len]);

        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
        let syntax = self
            .syntax_set
            .find_syntax_by_extension(ext)
            .or_else(|| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|name| self.syntax_set.find_syntax_by_token(name))
            })
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let text_preview = self.highlight_text(&text_slice, syntax, search_term, max_lines);

        Ok(PreviewResult::Text(text_preview))
    }

    /// Aplica realce de sintaxe linha por linha e identifica ocorrências do termo pesquisado.
    #[must_use]
    pub fn highlight_text(
        &self,
        text: &str,
        syntax: &SyntaxReference,
        search_term: Option<&str>,
        max_lines: usize,
    ) -> TextPreview {
        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut lines = Vec::new();
        let mut occurrences = Vec::new();

        let raw_lines: Vec<&str> = text.lines().collect();
        let total_lines = raw_lines.len();
        let limit = total_lines.min(max_lines);

        let search_lc = search_term.map(|s| s.to_lowercase());

        for (idx, &line_str) in raw_lines[..limit].iter().enumerate() {
            let lineno = idx + 1;
            let ranges = highlighter
                .highlight_line(line_str, &self.syntax_set)
                .unwrap_or_default();

            let mut spans = Vec::new();

            for (style, chunk) in ranges {
                let color = HighlightColor::from(style.foreground);

                if let Some(ref term) = search_lc {
                    if !term.is_empty() {
                        let chunk_lc = chunk.to_lowercase();
                        let mut last_idx = 0;

                        while let Some(found_idx) = chunk_lc[last_idx..].find(term) {
                            let match_start = last_idx + found_idx;
                            let match_end = match_start + term.len();

                            if match_start > last_idx {
                                spans.push(StyledSpan {
                                    text: chunk[last_idx..match_start].to_string(),
                                    color,
                                    is_match: false,
                                });
                            }

                            spans.push(StyledSpan {
                                text: chunk[match_start..match_end].to_string(),
                                color: HighlightColor {
                                    r: 255,
                                    g: 220,
                                    b: 0,
                                }, // destaque amarelo
                                is_match: true,
                            });

                            occurrences.push(SearchOccurrence {
                                line: lineno,
                                col: match_start + 1,
                                len: term.len(),
                            });

                            last_idx = match_end;
                        }

                        if last_idx < chunk.len() {
                            spans.push(StyledSpan {
                                text: chunk[last_idx..].to_string(),
                                color,
                                is_match: false,
                            });
                        }
                    } else {
                        spans.push(StyledSpan {
                            text: chunk.to_string(),
                            color,
                            is_match: false,
                        });
                    }
                } else {
                    spans.push(StyledSpan {
                        text: chunk.to_string(),
                        color,
                        is_match: false,
                    });
                }
            }

            lines.push(HighlightedLine { lineno, spans });
        }

        TextPreview {
            lines,
            total_lines,
            syntax_name: syntax.name.clone(),
            occurrences,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_binary() {
        assert!(!is_binary(b"Hello world\nThis is Rust code"));
        assert!(is_binary(b"\x00\x01\x02\x03\x04\x05"));
        assert!(is_binary(b"Some text with \x00 null byte in middle"));
    }

    #[test]
    fn test_format_hex_dump() {
        let bytes = b"FastFile binary test\x00\x01\x02";
        let dump = format_hex_dump(bytes, 64);
        assert!(!dump.is_empty());
        assert!(dump[0].contains("FastFile"));
    }

    #[test]
    fn test_preview_engine_highlight_text() {
        let engine = PreviewEngine::new();
        let code = "fn main() {\n    let answer = 42;\n    println!(\"{answer}\");\n}\n";
        let syntax = engine.syntax_set.find_syntax_by_extension("rs").unwrap();

        let preview = engine.highlight_text(code, syntax, Some("answer"), 100);
        assert_eq!(preview.total_lines, 4);
        assert_eq!(preview.syntax_name, "Rust");
        assert_eq!(preview.occurrences.len(), 2);
        assert_eq!(preview.occurrences[0].line, 2);
        assert_eq!(preview.occurrences[1].line, 3);
    }

    #[test]
    fn test_preview_file_with_mmap() {
        let engine = PreviewEngine::new();
        let tmp = std::env::temp_dir().join(format!("ff_test_prev_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);

        // 1. Arquivo de texto
        let text_file = tmp.join("test.rs");
        std::fs::write(&text_file, "pub struct Config {\n    pub port: u16,\n}\n").unwrap();

        let res = engine.preview_file(&text_file, Some("port"), 50).unwrap();
        match res {
            PreviewResult::Text(t) => {
                assert_eq!(t.occurrences.len(), 1);
                assert_eq!(t.occurrences[0].line, 2);
            }
            _ => panic!("Esperado Text preview"),
        }

        // 2. Arquivo binário
        let bin_file = tmp.join("test.bin");
        std::fs::write(
            &bin_file,
            b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00",
        )
        .unwrap();
        let res_bin = engine.preview_file(&bin_file, None, 50).unwrap();
        match res_bin {
            PreviewResult::Binary(b) => {
                assert_eq!(b.size, 16);
                assert!(!b.hex_dump.is_empty());
            }
            _ => panic!("Esperado Binary preview"),
        }

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

//! `ff-diff`
//!
//! Cálculo de diff estruturado entre arquivos e textos para o FastFile (§4, Quick Win #22 e Issue #19).

use std::fs::File;
use std::io;
use std::path::Path;

use memmap2::Mmap;
use serde::{Deserialize, Serialize};
use similar::{ChangeTag, DiffTag, TextDiff};

/// Tipo de alteração de linha ou trecho.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeKind {
    Equal,
    Insert,
    Delete,
    Modified,
}

/// Trecho de texto com sua classificação intra-linha.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffSpan {
    pub kind: ChangeKind,
    pub text: String,
}

/// Linha de diff com numeração e conteúdo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: ChangeKind,
    pub old_lineno: Option<usize>,
    pub new_lineno: Option<usize>,
    pub text: String,
    #[serde(default)]
    pub spans: Vec<DiffSpan>,
}

/// Bloco (hunk) contendo linhas de contexto e alterações.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffHunk {
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// Relatório consolidado de diferenças entre dois arquivos (formato linear unificado).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffReport {
    pub file_a: String,
    pub file_b: String,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
    pub is_identical: bool,
    #[serde(default)]
    pub is_binary: bool,
}

/// Célula de uma linha na visão lado a lado.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SideBySideCell {
    pub lineno: usize,
    pub text: String,
    pub spans: Vec<DiffSpan>,
}

/// Linha alinhada para renderização lado a lado (§4 e Issue #19).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SideBySideRow {
    pub left: Option<SideBySideCell>,
    pub right: Option<SideBySideCell>,
    pub kind: ChangeKind,
}

/// Estrutura completa de diff lado a lado com rolagem sincronizada.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SideBySideDiff {
    pub file_a: String,
    pub file_b: String,
    pub is_binary: bool,
    pub is_identical: bool,
    pub additions: usize,
    pub deletions: usize,
    pub rows: Vec<SideBySideRow>,
    /// Índices das linhas que contêm diferenças (para navegação rápida Próximo/Anterior).
    pub diff_indices: Vec<usize>,
}

/// Detecta se um buffer representa conteúdo binário (presença de bytes nulos nos primeiros 8 KB).
#[must_use]
pub fn is_binary(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(8192);
    bytes[..check_len].contains(&0)
}

/// Lê o conteúdo de um arquivo usando memória mapeada (`memmap2`) para arquivos grandes.
pub fn read_file_content<P: AsRef<Path>>(path: P) -> io::Result<(bool, String)> {
    let p = path.as_ref();
    let file = File::open(p)?;
    let meta = file.metadata()?;
    let len = meta.len();

    if len == 0 {
        return Ok((false, String::new()));
    }

    // Se o arquivo for muito pequeno (< 4KB), ler diretamente
    if len < 4096 {
        let bytes = std::fs::read(p)?;
        if is_binary(&bytes) {
            return Ok((true, String::new()));
        }
        return Ok((false, String::from_utf8_lossy(&bytes).into_owned()));
    }

    // Uso de memória mapeada conforme §4
    // SAFETY: o arquivo é aberto somente para leitura; assume-se que outros processos não o truncam durante o mapeamento (risco inerente a mmap).
    let mmap = unsafe { Mmap::map(&file)? };
    if is_binary(&mmap) {
        return Ok((true, String::new()));
    }

    let text = String::from_utf8_lossy(&mmap).into_owned();
    Ok((false, text))
}

/// Compara o conteúdo de dois arquivos do disco com suporte a mmap e detecção de binário.
pub fn diff_files<P: AsRef<Path>>(
    path_a: P,
    path_b: P,
    context_lines: usize,
) -> io::Result<DiffReport> {
    let p_a = path_a.as_ref();
    let p_b = path_b.as_ref();

    let (bin_a, content_a) = read_file_content(p_a)?;
    let (bin_b, content_b) = read_file_content(p_b)?;

    let name_a = p_a.to_string_lossy();
    let name_b = p_b.to_string_lossy();

    if bin_a || bin_b {
        return Ok(DiffReport {
            file_a: name_a.to_string(),
            file_b: name_b.to_string(),
            additions: 0,
            deletions: 0,
            hunks: Vec::new(),
            is_identical: false,
            is_binary: true,
        });
    }

    Ok(diff_texts(
        &name_a,
        &content_a,
        &name_b,
        &content_b,
        context_lines,
    ))
}

/// Realiza diff intra-linha entre duas versões de uma linha, identificando trechos modificados.
#[must_use]
pub fn diff_inline(old_text: &str, new_text: &str) -> (Vec<DiffSpan>, Vec<DiffSpan>) {
    let diff = TextDiff::from_words(old_text, new_text);
    let mut left_spans = Vec::new();
    let mut right_spans = Vec::new();

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                let val = change.value().to_string();
                left_spans.push(DiffSpan {
                    kind: ChangeKind::Equal,
                    text: val.clone(),
                });
                right_spans.push(DiffSpan {
                    kind: ChangeKind::Equal,
                    text: val,
                });
            }
            ChangeTag::Delete => {
                left_spans.push(DiffSpan {
                    kind: ChangeKind::Delete,
                    text: change.value().to_string(),
                });
            }
            ChangeTag::Insert => {
                right_spans.push(DiffSpan {
                    kind: ChangeKind::Insert,
                    text: change.value().to_string(),
                });
            }
        }
    }

    (left_spans, right_spans)
}

/// Compara duas strings em memória e produz o relatório estruturado unificado.
#[must_use]
pub fn diff_texts(
    name_a: &str,
    text_a: &str,
    name_b: &str,
    text_b: &str,
    context_lines: usize,
) -> DiffReport {
    let diff = TextDiff::from_lines(text_a, text_b);

    let mut additions = 0;
    let mut deletions = 0;
    let mut hunks = Vec::new();

    for group in diff.grouped_ops(context_lines) {
        if group.is_empty() {
            continue;
        }

        let old_start = group.first().map(|op| op.old_range().start).unwrap_or(0);
        let old_end = group.last().map(|op| op.old_range().end).unwrap_or(0);
        let old_lines = old_end.saturating_sub(old_start);

        let new_start = group.first().map(|op| op.new_range().start).unwrap_or(0);
        let new_end = group.last().map(|op| op.new_range().end).unwrap_or(0);
        let new_lines = new_end.saturating_sub(new_start);

        let header = format!(
            "@@ -{},{} +{},{} @@",
            old_start + 1,
            old_lines,
            new_start + 1,
            new_lines
        );

        let mut lines = Vec::new();

        for op in &group {
            for change in diff.iter_changes(op) {
                let (kind, old_lineno, new_lineno) = match change.tag() {
                    ChangeTag::Equal => (ChangeKind::Equal, change.old_index(), change.new_index()),
                    ChangeTag::Delete => {
                        deletions += 1;
                        (ChangeKind::Delete, change.old_index(), None)
                    }
                    ChangeTag::Insert => {
                        additions += 1;
                        (ChangeKind::Insert, None, change.new_index())
                    }
                };

                let text = change.value().trim_end_matches(['\r', '\n']).to_string();

                lines.push(DiffLine {
                    kind,
                    old_lineno: old_lineno.map(|i| i + 1),
                    new_lineno: new_lineno.map(|i| i + 1),
                    text,
                    spans: Vec::new(),
                });
            }
        }

        hunks.push(DiffHunk {
            old_start: old_start + 1,
            old_lines,
            new_start: new_start + 1,
            new_lines,
            header,
            lines,
        });
    }

    let is_identical = additions == 0 && deletions == 0;

    DiffReport {
        file_a: name_a.to_string(),
        file_b: name_b.to_string(),
        additions,
        deletions,
        hunks,
        is_identical,
        is_binary: false,
    }
}

/// Compara dois arquivos e produz a estrutura para visualização lado a lado (Side-by-Side).
pub fn diff_files_side_by_side<P: AsRef<Path>>(path_a: P, path_b: P) -> io::Result<SideBySideDiff> {
    let p_a = path_a.as_ref();
    let p_b = path_b.as_ref();

    let (bin_a, text_a) = read_file_content(p_a)?;
    let (bin_b, text_b) = read_file_content(p_b)?;

    let name_a = p_a.to_string_lossy().to_string();
    let name_b = p_b.to_string_lossy().to_string();

    if bin_a || bin_b {
        return Ok(SideBySideDiff {
            file_a: name_a,
            file_b: name_b,
            is_binary: true,
            is_identical: false,
            additions: 0,
            deletions: 0,
            rows: Vec::new(),
            diff_indices: Vec::new(),
        });
    }

    Ok(diff_side_by_side(&name_a, &text_a, &name_b, &text_b))
}

/// Gera o diff lado a lado alinhado entre duas strings completas (§4 e Issue #19).
#[must_use]
pub fn diff_side_by_side(name_a: &str, text_a: &str, name_b: &str, text_b: &str) -> SideBySideDiff {
    let diff = TextDiff::from_lines(text_a, text_b);

    let mut rows: Vec<SideBySideRow> = Vec::new();
    let mut diff_indices: Vec<usize> = Vec::new();
    let mut additions = 0;
    let mut deletions = 0;

    let mut left_lineno = 1usize;
    let mut right_lineno = 1usize;

    for op in diff.ops() {
        match op.tag() {
            DiffTag::Equal => {
                for change in diff.iter_changes(op) {
                    let text = change.value().trim_end_matches(['\r', '\n']).to_string();
                    let spans = vec![DiffSpan {
                        kind: ChangeKind::Equal,
                        text: text.clone(),
                    }];

                    rows.push(SideBySideRow {
                        left: Some(SideBySideCell {
                            lineno: left_lineno,
                            text: text.clone(),
                            spans: spans.clone(),
                        }),
                        right: Some(SideBySideCell {
                            lineno: right_lineno,
                            text,
                            spans,
                        }),
                        kind: ChangeKind::Equal,
                    });

                    left_lineno += 1;
                    right_lineno += 1;
                }
            }
            DiffTag::Delete => {
                for change in diff.iter_changes(op) {
                    let text = change.value().trim_end_matches(['\r', '\n']).to_string();
                    deletions += 1;
                    let idx = rows.len();
                    diff_indices.push(idx);

                    rows.push(SideBySideRow {
                        left: Some(SideBySideCell {
                            lineno: left_lineno,
                            text: text.clone(),
                            spans: vec![DiffSpan {
                                kind: ChangeKind::Delete,
                                text,
                            }],
                        }),
                        right: None,
                        kind: ChangeKind::Delete,
                    });

                    left_lineno += 1;
                }
            }
            DiffTag::Insert => {
                for change in diff.iter_changes(op) {
                    let text = change.value().trim_end_matches(['\r', '\n']).to_string();
                    additions += 1;
                    let idx = rows.len();
                    diff_indices.push(idx);

                    rows.push(SideBySideRow {
                        left: None,
                        right: Some(SideBySideCell {
                            lineno: right_lineno,
                            text: text.clone(),
                            spans: vec![DiffSpan {
                                kind: ChangeKind::Insert,
                                text,
                            }],
                        }),
                        kind: ChangeKind::Insert,
                    });

                    right_lineno += 1;
                }
            }
            DiffTag::Replace => {
                for change in diff.iter_changes(op) {
                    let text = change.value().trim_end_matches(['\r', '\n']).to_string();
                    match change.tag() {
                        ChangeTag::Delete => {
                            deletions += 1;
                            let idx = rows.len();
                            diff_indices.push(idx);

                            rows.push(SideBySideRow {
                                left: Some(SideBySideCell {
                                    lineno: left_lineno,
                                    text: text.clone(),
                                    spans: vec![DiffSpan {
                                        kind: ChangeKind::Delete,
                                        text,
                                    }],
                                }),
                                right: None,
                                kind: ChangeKind::Delete,
                            });

                            left_lineno += 1;
                        }
                        ChangeTag::Insert => {
                            additions += 1;
                            let idx = rows.len();
                            diff_indices.push(idx);

                            rows.push(SideBySideRow {
                                left: None,
                                right: Some(SideBySideCell {
                                    lineno: right_lineno,
                                    text: text.clone(),
                                    spans: vec![DiffSpan {
                                        kind: ChangeKind::Insert,
                                        text,
                                    }],
                                }),
                                kind: ChangeKind::Insert,
                            });

                            right_lineno += 1;
                        }
                        ChangeTag::Equal => {}
                    }
                }
            }
        }
    }

    // Pós-processamento para alinhar linhas modificadas (pares de Delete e Insert adjacentes)
    // aplicando diff intra-linha
    let mut aligned_rows: Vec<SideBySideRow> = Vec::with_capacity(rows.len());
    let mut aligned_diff_indices: Vec<usize> = Vec::new();

    let mut i = 0;
    while i < rows.len() {
        if rows[i].kind == ChangeKind::Delete {
            // Verificar se temos uma sequência de deletes seguida por inserts
            let mut deletes = Vec::new();
            while i < rows.len() && rows[i].kind == ChangeKind::Delete {
                deletes.push(rows[i].clone());
                i += 1;
            }

            let mut inserts = Vec::new();
            while i < rows.len() && rows[i].kind == ChangeKind::Insert {
                inserts.push(rows[i].clone());
                i += 1;
            }

            let max_pairs = deletes.len().min(inserts.len());
            for p in 0..max_pairs {
                let left_cell = deletes[p].left.as_ref().unwrap();
                let right_cell = inserts[p].right.as_ref().unwrap();

                // Calcula diff intra-linha com similar
                let (left_spans, right_spans) = diff_inline(&left_cell.text, &right_cell.text);

                let row_idx = aligned_rows.len();
                aligned_diff_indices.push(row_idx);

                aligned_rows.push(SideBySideRow {
                    left: Some(SideBySideCell {
                        lineno: left_cell.lineno,
                        text: left_cell.text.clone(),
                        spans: left_spans,
                    }),
                    right: Some(SideBySideCell {
                        lineno: right_cell.lineno,
                        text: right_cell.text.clone(),
                        spans: right_spans,
                    }),
                    kind: ChangeKind::Modified,
                });
            }

            // Deletes restantes sem par
            for del in deletes.iter().skip(max_pairs) {
                let row_idx = aligned_rows.len();
                aligned_diff_indices.push(row_idx);
                aligned_rows.push(del.clone());
            }

            // Inserts restantes sem par
            for ins in inserts.iter().skip(max_pairs) {
                let row_idx = aligned_rows.len();
                aligned_diff_indices.push(row_idx);
                aligned_rows.push(ins.clone());
            }
        } else {
            if rows[i].kind != ChangeKind::Equal {
                aligned_diff_indices.push(aligned_rows.len());
            }
            aligned_rows.push(rows[i].clone());
            i += 1;
        }
    }

    let is_identical = additions == 0 && deletions == 0;

    SideBySideDiff {
        file_a: name_a.to_string(),
        file_b: name_b.to_string(),
        is_binary: false,
        is_identical,
        additions,
        deletions,
        rows: aligned_rows,
        diff_indices: aligned_diff_indices,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_texts() {
        let text = "alpha\nbeta\ngamma\n";
        let report = diff_texts("a.txt", text, "b.txt", text, 3);
        assert!(report.is_identical);
        assert_eq!(report.additions, 0);
        assert_eq!(report.deletions, 0);
        assert!(report.hunks.is_empty());
    }

    #[test]
    fn test_modified_texts() {
        let a = "version = \"1.0.0\"\nname = \"fastfile\"\n";
        let b = "version = \"1.1.0\"\nname = \"fastfile\"\nauthor = \"Fabio\"\n";

        let report = diff_texts("old.toml", a, "new.toml", b, 2);
        assert!(!report.is_identical);
        assert_eq!(report.deletions, 1);
        assert_eq!(report.additions, 2);
        assert_eq!(report.hunks.len(), 1);

        let hunk = &report.hunks[0];
        assert_eq!(hunk.lines[0].kind, ChangeKind::Delete);
        assert_eq!(hunk.lines[0].text, "version = \"1.0.0\"");
        assert_eq!(hunk.lines[1].kind, ChangeKind::Insert);
        assert_eq!(hunk.lines[1].text, "version = \"1.1.0\"");
        assert_eq!(hunk.lines[2].kind, ChangeKind::Equal);
        assert_eq!(hunk.lines[3].kind, ChangeKind::Insert);
        assert_eq!(hunk.lines[3].text, "author = \"Fabio\"");
    }

    #[test]
    fn test_side_by_side_diff_and_intra_line() {
        let a = "fn calculate(x: i32) -> i32 {\n    x + 10\n}\n";
        let b = "fn calculate(x: i64) -> i64 {\n    x + 10\n}\n";

        let sbs = diff_side_by_side("old.rs", a, "new.rs", b);
        assert!(!sbs.is_identical);
        assert_eq!(sbs.rows.len(), 3);

        // A primeira linha deve ser Modified com spans intra-linha
        let row0 = &sbs.rows[0];
        assert_eq!(row0.kind, ChangeKind::Modified);
        assert!(row0.left.is_some());
        assert!(row0.right.is_some());

        let left_cell = row0.left.as_ref().unwrap();
        let right_cell = row0.right.as_ref().unwrap();
        assert_eq!(left_cell.lineno, 1);
        assert_eq!(right_cell.lineno, 1);

        // Deve conter spans de Delete e Insert
        assert!(left_cell
            .spans
            .iter()
            .any(|s| s.kind == ChangeKind::Delete && s.text.contains("i32")));
        assert!(right_cell
            .spans
            .iter()
            .any(|s| s.kind == ChangeKind::Insert && s.text.contains("i64")));

        // Linha 2 e 3 são iguais
        assert_eq!(sbs.rows[1].kind, ChangeKind::Equal);
        assert_eq!(sbs.rows[2].kind, ChangeKind::Equal);

        // diff_indices deve apontar apenas para a linha 0
        assert_eq!(sbs.diff_indices, vec![0]);
    }

    #[test]
    fn test_binary_detection() {
        let text_bytes = b"Hello, world! This is a plain text file.\n";
        assert!(!is_binary(text_bytes));

        let bin_bytes = b"\x7fELF\x02\x01\x01\x00\x00\x00\x00\x00";
        assert!(is_binary(bin_bytes));
    }

    #[test]
    fn test_diff_files_with_mmap() {
        let tmp = std::env::temp_dir().join(format!("ff_test_diff_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&tmp);
        let f1 = tmp.join("file1.txt");
        let f2 = tmp.join("file2.txt");

        std::fs::write(&f1, "Line 1\nLine 2\nLine 3\n").unwrap();
        std::fs::write(&f2, "Line 1\nLine 2 modified\nLine 3\n").unwrap();

        let sbs = diff_files_side_by_side(&f1, &f2).unwrap();
        assert!(!sbs.is_identical);
        assert!(!sbs.is_binary);
        assert_eq!(sbs.rows.len(), 3);
        assert_eq!(sbs.rows[1].kind, ChangeKind::Modified);

        let _ = std::fs::remove_dir_all(&tmp);
    }
}

//! `ff-diff`
//!
//! Cálculo de diff estruturado entre arquivos e textos para o FastFile (§4 e Quick Win #22).

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use similar::{ChangeTag, TextDiff};

/// Tipo de alteração de linha ou trecho.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeKind {
    Equal,
    Insert,
    Delete,
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

/// Relatório consolidado de diferenças entre dois arquivos.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiffReport {
    pub file_a: String,
    pub file_b: String,
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
    pub is_identical: bool,
}

/// Compara o conteúdo de dois arquivos do disco.
pub fn diff_files<P: AsRef<Path>>(
    path_a: P,
    path_b: P,
    context_lines: usize,
) -> io::Result<DiffReport> {
    let p_a = path_a.as_ref();
    let p_b = path_b.as_ref();

    let content_a = fs::read_to_string(p_a)?;
    let content_b = fs::read_to_string(p_b)?;

    Ok(diff_texts(
        &p_a.to_string_lossy(),
        &content_a,
        &p_b.to_string_lossy(),
        &content_b,
        context_lines,
    ))
}

/// Compara duas strings em memória e produz o relatório estruturado.
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
}

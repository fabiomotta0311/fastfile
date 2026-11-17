//! Definição do esquema JSON versionado `ff/1` (§3.1, §3.2).

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: &str = "ff/1";
pub const DEFAULT_SOURCE: &str = "walk";

/// Envelope padrão para respostas JSON bem-sucedidas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonEnvelope<T> {
    pub schema: String,
    pub source: String,
    pub command: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    pub summary: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

impl<T> JsonEnvelope<T> {
    pub fn new(command: &str, summary: serde_json::Value) -> Self {
        Self {
            schema: SCHEMA_VERSION.to_string(),
            source: DEFAULT_SOURCE.to_string(),
            command: command.to_string(),
            warnings: Vec::new(),
            summary,
            results: None,
            result: None,
            cursor: None,
            has_more: None,
            truncated: None,
        }
    }

    #[must_use]
    pub fn with_results(mut self, results: T) -> Self {
        self.results = Some(results);
        self
    }

    #[must_use]
    pub fn with_result(mut self, result: T) -> Self {
        self.result = Some(result);
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    #[must_use]
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }

    #[must_use]
    pub fn with_cursor(mut self, cursor: Option<String>, has_more: bool) -> Self {
        self.cursor = cursor;
        self.has_more = if has_more { Some(true) } else { None };
        self
    }

    #[must_use]
    pub fn with_truncated(mut self, truncated: bool) -> Self {
        if truncated {
            self.truncated = Some(true);
        }
        self
    }
}

/// Finaliza o envelope aplicando o orçamento de tokens sobre toda a resposta serializada (§3.1, §3.2).
///
/// Unidade: tokens estimados onde 1 token ≈ 4 bytes UTF-8 (`max_bytes = budget * 4`).
///
/// - Contabiliza o envelope inteiro (schema, source, command, summary, warnings, results, etc.).
/// - Se nem o envelope mínimo (com `results: []`) couber no orçamento, esvazia `results`, marca
///   `truncated: true`, `has_more: true` (se houver itens) e inclui aviso diagnóstico.
/// - Se couber, inclui o máximo de itens possíveis sem ultrapassar `max_bytes`.
/// - Retorna `(envelope_final, json_serializado, foi_truncado)`.
pub fn finalize_envelope_budget<T: Serialize + Clone>(
    mut env: JsonEnvelope<Vec<T>>,
    budget: Option<usize>,
    compact: bool,
    total_matched: usize,
    offset: usize,
) -> (JsonEnvelope<Vec<T>>, String, bool) {
    let all_items = env.results.take().unwrap_or_default();
    let original_count = all_items.len();

    let serialize = |e: &JsonEnvelope<Vec<T>>| -> String {
        if compact {
            serde_json::to_string(e).unwrap_or_default()
        } else {
            serde_json::to_string_pretty(e).unwrap_or_default()
        }
    };

    let Some(max_tokens) = budget else {
        let next_cursor = if offset + all_items.len() < total_matched {
            Some((offset + all_items.len()).to_string())
        } else {
            None
        };
        let has_more = next_cursor.is_some();
        env.results = Some(all_items);
        env.cursor = next_cursor;
        env.has_more = if has_more { Some(true) } else { None };
        let json_str = serialize(&env);
        return (env, json_str, false);
    };

    let max_bytes = max_tokens.saturating_mul(4);

    // Testar envelope mínimo com resultados vazios []
    env.results = Some(Vec::new());
    env.truncated = Some(true);
    let has_more_on_trunc = (offset + original_count) < total_matched || original_count > 0;
    env.has_more = if has_more_on_trunc { Some(true) } else { None };
    env.cursor = if has_more_on_trunc {
        Some(offset.to_string())
    } else {
        None
    };

    let min_str = serialize(&env);
    if min_str.len() > max_bytes {
        // Nem o envelope mínimo cabe no orçamento solicitado
        env.warnings.push(format!(
            "orçamento de {max_tokens} token(s) (~{max_bytes} bytes) insuficiente para incluir resultados (envelope mínimo requer {} bytes); resultados omitidos",
            min_str.len()
        ));
        let final_str = serialize(&env);
        return (env, final_str, true);
    }

    // Envelope mínimo coube! Tenta incluir itens incrementalmente
    let mut kept: Vec<T> = Vec::new();
    let mut last_valid_str = min_str;
    let mut truncated_by_budget = false;

    for item in all_items {
        kept.push(item);
        env.results = Some(kept.clone());
        let next_cursor = if offset + kept.len() < total_matched {
            Some((offset + kept.len()).to_string())
        } else {
            None
        };
        let has_more = next_cursor.is_some();
        env.cursor = next_cursor;
        env.has_more = if has_more { Some(true) } else { None };
        env.truncated = None;

        let candidate_str = serialize(&env);
        if candidate_str.len() > max_bytes {
            kept.pop();
            truncated_by_budget = true;
            break;
        } else {
            last_valid_str = candidate_str;
        }
    }

    if truncated_by_budget || kept.len() < original_count {
        env.results = Some(kept);
        env.truncated = Some(true);
        let next_offset = offset + env.results.as_ref().map(|r| r.len()).unwrap_or(0);
        let has_more = next_offset < total_matched || original_count > 0;
        env.has_more = if has_more { Some(true) } else { None };
        env.cursor = if has_more {
            Some(next_offset.to_string())
        } else {
            None
        };
        env.warnings
            .push("limite de tokens (--budget) atingido; resultados truncados".to_string());

        let mut final_str = serialize(&env);
        // Se a adição do warning ultrapassou max_bytes, descarta itens até caber
        while final_str.len() > max_bytes && !env.results.as_ref().unwrap().is_empty() {
            env.results.as_mut().unwrap().pop();
            let cur_len = env.results.as_ref().unwrap().len();
            let next_offset = offset + cur_len;
            env.cursor = Some(next_offset.to_string());
            final_str = serialize(&env);
        }

        if final_str.len() > max_bytes {
            env.warnings.pop();
            final_str = serialize(&env);
        }
        (env, final_str, true)
    } else {
        env.results = Some(kept);
        (env, last_valid_str, false)
    }
}

/// Envelope estruturado para relatar erros em JSON.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonErrorEnvelope {
    pub schema: String,
    pub error: JsonErrorDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonErrorDetails {
    pub code: String,
    pub message: String,
}

impl JsonErrorEnvelope {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            schema: SCHEMA_VERSION.to_string(),
            error: JsonErrorDetails {
                code: code.to_string(),
                message: message.to_string(),
            },
        }
    }
}

/// Item de resultado retornado para `ff find` e `ff recent`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileResultItem {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtime_iso: Option<String>,
}

impl FileResultItem {
    #[must_use]
    pub fn name_or_basename(&self) -> &str {
        if let Some(ref n) = self.name {
            n.as_str()
        } else {
            std::path::Path::new(&self.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&self.path)
        }
    }
}

/// Item de resultado retornado para `ff du`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DuResultItem {
    pub path: String,
    pub size: u64,
    pub files: u64,
    pub dirs: u64,
    pub depth: u32,
}

/// Resultado detalhado para `ff stat`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatResult {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub size: u64,
    pub mtime: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime_iso: Option<String>,
    pub readonly: bool,
    pub hidden: bool,
    pub system: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_files: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_dirs: Option<u64>,
}

/// Item de resultado retornado para `ff sym`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SymbolResultItem {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub signature: String,
    pub language: String,
}

/// Visão compacta de um repositório para agentes (`ff map`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepoMapResult {
    pub root: String,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_bytes: u64,
    pub languages: Vec<LanguageStat>,
    pub modules: Vec<ModuleSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tree: Vec<MapTreeNode>,
}

/// Estatísticas de uma linguagem no repositório.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LanguageStat {
    pub language: String,
    pub files: usize,
    pub bytes: u64,
    pub percentage: f64,
}

/// Resumo de um módulo ou subprojeto no repositório.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModuleSummary {
    pub path: String,
    pub name: String,
    pub files: usize,
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub primary_files: Vec<MapFileEntry>,
}

/// Arquivo primário ou relevante dentro de um módulo.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapFileEntry {
    pub path: String,
    pub name: String,
    pub size: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<String>,
}

/// Nó na árvore compacta de arquivos e pastas.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MapTreeNode {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub depth: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<String>,
}

/// Grupo de arquivos com conteúdo idêntico (`ff dupes`, §3.5 e #28).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DupeGroup {
    pub size: u64,
    pub hash: String,
    pub count: usize,
    pub wasted_bytes: u64,
    pub files: Vec<String>,
}

/// Resultado consolidado da busca por arquivos duplicados.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DupesResult {
    pub target: String,
    pub total_scanned_files: usize,
    pub candidate_files: usize,
    pub duplicate_groups: usize,
    pub duplicate_files: usize,
    pub wasted_bytes: u64,
    pub groups: Vec<DupeGroup>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_accounts_for_truncation_warning() {
        let env = JsonEnvelope::new("find", serde_json::json!({}))
            .with_results(vec!["a".to_string(), "b".repeat(500)]);
        let (_, json, truncated) = finalize_envelope_budget(env, Some(45), true, 2, 0);
        assert!(truncated);
        assert!(json.len() <= 180, "{} bytes: {}", json.len(), json);
    }

    #[test]
    fn test_finalize_envelope_budget_zero_items_on_budget_one() {
        let items = vec![
            FileResultItem {
                path: "src/main.rs".into(),
                name: Some("main.rs".into()),
                is_dir: false,
                size: 1024,
                mtime: 1700000000,
                mtime_iso: Some("2023-11-14T22:13:20Z".into()),
            },
            FileResultItem {
                path: "src/lib.rs".into(),
                name: Some("lib.rs".into()),
                is_dir: false,
                size: 2048,
                mtime: 1700000000,
                mtime_iso: Some("2023-11-14T22:13:20Z".into()),
            },
        ];

        let env = JsonEnvelope::new("find", serde_json::json!({"total": 2})).with_results(items);

        // Budget de 1 token (~4 bytes): nem o envelope mínimo cabe
        let (res_env, json_str, truncated) = finalize_envelope_budget(env, Some(1), true, 2, 0);

        assert!(truncated);
        assert_eq!(res_env.results.unwrap().len(), 0);
        assert_eq!(res_env.truncated, Some(true));
        assert_eq!(res_env.has_more, Some(true));
        assert_eq!(res_env.cursor, Some("0".into()));
        assert!(!res_env.warnings.is_empty());
        assert!(res_env.warnings[0].contains("insuficiente"));
        // O JSON retornado NÃO deve conter itens e deve ser muito menor que 881 bytes
        assert!(json_str.len() < 350);
    }

    #[test]
    fn test_finalize_envelope_budget_fits_partial_items() {
        let items: Vec<DuResultItem> = (0..10)
            .map(|i| DuResultItem {
                path: format!("crates/mod_{i}"),
                size: (i as u64 + 1) * 1000,
                files: 5,
                dirs: 1,
                depth: 1,
            })
            .collect();

        let env = JsonEnvelope::new("du", serde_json::json!({"target": "."})).with_results(items);

        // Budget de 80 tokens (~320 bytes em modo compacto)
        let (res_env, json_str, truncated) = finalize_envelope_budget(env, Some(80), true, 10, 0);

        assert!(truncated);
        let count = res_env.results.unwrap().len();
        assert!(count > 0 && count < 10);
        assert!(json_str.len() <= 320);
        assert_eq!(res_env.truncated, Some(true));
        assert_eq!(res_env.has_more, Some(true));
        assert_eq!(res_env.cursor, Some(count.to_string()));
    }
}

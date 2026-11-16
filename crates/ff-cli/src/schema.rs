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
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub mtime: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mtime_iso: Option<String>,
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

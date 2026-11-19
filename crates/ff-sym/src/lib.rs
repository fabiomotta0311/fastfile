//! Extração e busca de símbolos no código-fonte via Tree-Sitter (§3.5 e #25).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::SystemTime;

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

/// Tipo de símbolo de código.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Method,
    Struct,
    Class,
    Trait,
    Interface,
    Enum,
    TypeAlias,
    Constant,
    Module,
    Other,
}

impl SymbolKind {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Struct => "struct",
            Self::Class => "class",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::TypeAlias => "type",
            Self::Constant => "constant",
            Self::Module => "module",
            Self::Other => "other",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let lower = s.to_lowercase();
        match lower.as_str() {
            "fn" | "function" | "func" | "def" => Some(Self::Function),
            "method" => Some(Self::Method),
            "struct" => Some(Self::Struct),
            "class" => Some(Self::Class),
            "trait" => Some(Self::Trait),
            "interface" => Some(Self::Interface),
            "enum" => Some(Self::Enum),
            "type" | "typedef" | "alias" => Some(Self::TypeAlias),
            "const" | "constant" => Some(Self::Constant),
            "mod" | "module" => Some(Self::Module),
            _ => None,
        }
    }
}

/// Representação de um símbolo extraído do código-fonte.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    /// Nome do identificador (ex.: `AuthenticatedExecutor`, `run_mcp_server`).
    pub name: String,
    /// Categoria do símbolo (função, struct, classe, etc.).
    pub kind: SymbolKind,
    /// Caminho do arquivo contendo o símbolo.
    pub path: String,
    /// Linha de declaração (base 1).
    pub line: usize,
    /// Coluna de declaração (base 1).
    pub column: usize,
    /// Linha ou assinatura resumida da declaração.
    pub signature: String,
    /// Linguagem do arquivo (rust, python, typescript, etc.).
    pub language: String,
}

/// Linguagens suportadas nativamente pelo `ff sym`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupportedLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Go,
    CSharp,
}

impl SupportedLanguage {
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        Self::from_extension(&ext)
    }

    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "rs" => Some(Self::Rust),
            "py" | "pyi" => Some(Self::Python),
            "js" | "mjs" | "cjs" | "jsx" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "go" => Some(Self::Go),
            "cs" => Some(Self::CSharp),
            _ => None,
        }
    }

    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::Go => "go",
            Self::CSharp => "csharp",
        }
    }

    #[must_use]
    pub fn tree_sitter_language(&self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        }
    }
}

/// Cache de símbolos indexados com invalidação por data de modificação.
pub struct SymbolCache {
    entries: RwLock<HashMap<PathBuf, (SystemTime, Vec<Symbol>)>>,
}

impl Default for SymbolCache {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolCache {
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Obtém os símbolos do arquivo, reutilizando cache se a data de modificação coincidir.
    pub fn get_or_parse(&self, path: &Path, content: &str, mtime: SystemTime) -> Vec<Symbol> {
        if let Ok(guard) = self.entries.read() {
            if let Some((cached_mtime, symbols)) = guard.get(path) {
                if *cached_mtime == mtime {
                    return symbols.clone();
                }
            }
        }

        let symbols = extract_symbols(path, content);

        if let Ok(mut guard) = self.entries.write() {
            guard.insert(path.to_path_buf(), (mtime, symbols.clone()));
        }

        symbols
    }
}

/// Extrai símbolos de um arquivo a partir de seu conteúdo e extensão.
#[must_use]
pub fn extract_symbols(path: &Path, content: &str) -> Vec<Symbol> {
    let Some(lang) = SupportedLanguage::from_path(path) else {
        return Vec::new();
    };

    let mut parser = Parser::new();
    if parser.set_language(&lang.tree_sitter_language()).is_err() {
        return Vec::new();
    }

    let Some(tree) = parser.parse(content, None) else {
        return Vec::new();
    };

    let mut symbols = Vec::new();
    let path_str = path.to_string_lossy().to_string();
    let lines: Vec<&str> = content.lines().collect();

    collect_symbols_from_node(
        tree.root_node(),
        content.as_bytes(),
        &lines,
        &path_str,
        lang,
        &mut symbols,
    );

    symbols
}

fn collect_symbols_from_node(
    node: Node,
    source: &[u8],
    lines: &[&str],
    path: &str,
    lang: SupportedLanguage,
    results: &mut Vec<Symbol>,
) {
    if let Some((name, kind)) = identify_definition(node, source, lang) {
        let start_point = node.start_position();
        let line_idx = start_point.row;
        let line_num = line_idx + 1;
        let col_num = start_point.column + 1;

        let signature = if line_idx < lines.len() {
            lines[line_idx].trim().to_string()
        } else {
            name.clone()
        };

        results.push(Symbol {
            name,
            kind,
            path: path.to_string(),
            line: line_num,
            column: col_num,
            signature,
            language: lang.name().to_string(),
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_symbols_from_node(child, source, lines, path, lang, results);
    }
}

/// Metadados sintáticos locais, sem resolução de nomes ou expansão de macros.
pub struct RustSite {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub module: usize,
    pub is_test: bool,
}

/// Extrai os mesmos tipos de definição de `sym`, com faixas e testes `#[test]`.
pub fn rust_sites(source: &str) -> Option<(Vec<RustSite>, bool)> {
    let mut parser = Parser::new();
    parser
        .set_language(&SupportedLanguage::Rust.tree_sitter_language())
        .ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let mut sites = Vec::new();
    let mut stack = vec![(root, 0)];
    while let Some((node, module)) = stack.pop() {
        if let Some((name, kind)) =
            identify_definition(node, source.as_bytes(), SupportedLanguage::Rust)
        {
            let mut sibling = (kind == SymbolKind::Function)
                .then(|| node.prev_named_sibling())
                .flatten();
            let mut is_test = false;
            while let Some(attr) = sibling {
                match attr.kind() {
                    "attribute_item" => {
                        is_test |= attr
                            .named_child(0)
                            .and_then(|a| a.named_child(0))
                            .is_some_and(|name| {
                                name.kind() == "identifier"
                                    && source.get(name.byte_range()) == Some("test")
                            });
                    }
                    "line_comment" | "block_comment" => {}
                    _ => break,
                }
                sibling = attr.prev_named_sibling();
            }
            sites.push(RustSite {
                name,
                start: node.start_position().row + 1,
                end: node.end_position().row + 1,
                module,
                is_test,
            });
        }
        let module = if node.kind() == "mod_item" {
            node.start_byte() + 1
        } else {
            module
        };
        let mut cursor = node.walk();
        stack.extend(
            node.named_children(&mut cursor)
                .map(|child| (child, module)),
        );
    }
    sites.sort_by_key(|site| site.start);
    Some((sites, root.has_error()))
}

fn identify_definition(
    node: Node,
    source: &[u8],
    lang: SupportedLanguage,
) -> Option<(String, SymbolKind)> {
    let kind = node.kind();

    match lang {
        SupportedLanguage::Rust => match kind {
            "function_item" | "function_signature_item" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Function))
            }
            "struct_item" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Struct))
            }
            "enum_item" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Enum))
            }
            "trait_item" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Trait))
            }
            "type_item" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::TypeAlias))
            }
            "const_item" | "static_item" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Constant))
            }
            "mod_item" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Module))
            }
            _ => None,
        },
        SupportedLanguage::Python => match kind {
            "function_definition" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Function))
            }
            "class_definition" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Class))
            }
            _ => None,
        },
        SupportedLanguage::JavaScript | SupportedLanguage::TypeScript | SupportedLanguage::Tsx => {
            match kind {
                "function_declaration" | "generator_function_declaration" => {
                    let name_node = node.child_by_field_name("name")?;
                    Some((get_node_text(name_node, source)?, SymbolKind::Function))
                }
                "method_definition" => {
                    let name_node = node.child_by_field_name("name")?;
                    Some((get_node_text(name_node, source)?, SymbolKind::Method))
                }
                "class_declaration" => {
                    let name_node = node.child_by_field_name("name")?;
                    Some((get_node_text(name_node, source)?, SymbolKind::Class))
                }
                "interface_declaration" => {
                    let name_node = node.child_by_field_name("name")?;
                    Some((get_node_text(name_node, source)?, SymbolKind::Interface))
                }
                "type_alias_declaration" => {
                    let name_node = node.child_by_field_name("name")?;
                    Some((get_node_text(name_node, source)?, SymbolKind::TypeAlias))
                }
                "enum_declaration" => {
                    let name_node = node.child_by_field_name("name")?;
                    Some((get_node_text(name_node, source)?, SymbolKind::Enum))
                }
                _ => None,
            }
        }
        SupportedLanguage::Go => match kind {
            "function_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Function))
            }
            "method_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Method))
            }
            "type_spec" => {
                let name_node = node.child_by_field_name("name")?;
                let type_node = node.child_by_field_name("type");
                let sym_kind = if let Some(t) = type_node {
                    match t.kind() {
                        "struct_type" => SymbolKind::Struct,
                        "interface_type" => SymbolKind::Interface,
                        _ => SymbolKind::TypeAlias,
                    }
                } else {
                    SymbolKind::TypeAlias
                };
                Some((get_node_text(name_node, source)?, sym_kind))
            }
            _ => None,
        },
        SupportedLanguage::CSharp => match kind {
            "class_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Class))
            }
            "struct_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Struct))
            }
            "interface_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Interface))
            }
            "enum_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Enum))
            }
            "method_declaration" => {
                let name_node = node.child_by_field_name("name")?;
                Some((get_node_text(name_node, source)?, SymbolKind::Method))
            }
            _ => None,
        },
    }
}

fn get_node_text(node: Node, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}

/// Parâmetros de busca de símbolos.
#[derive(Debug, Clone)]
pub struct SymOptions {
    /// Nome ou padrão do símbolo a buscar (ex.: `AuthenticatedExecutor`).
    pub name: Option<String>,
    /// Caminho base da busca (diretório ou arquivo único).
    pub path: PathBuf,
    /// Filtro de tipo de símbolo (fn, struct, class, etc.).
    pub kind: Option<SymbolKind>,
    /// Filtro de linguagem (rust, python, ts, etc.).
    pub lang: Option<String>,
    /// Limite máximo de resultados a retornar.
    pub limit: usize,
    /// Incluir pastas ignoradas (.git, target, node_modules).
    pub all: bool,
}

impl Default for SymOptions {
    fn default() -> Self {
        Self {
            name: None,
            path: PathBuf::from("."),
            kind: None,
            lang: None,
            limit: 50,
            all: false,
        }
    }
}

/// Motor de busca de símbolos no repositório.
pub struct SymbolEngine {
    cache: SymbolCache,
}

impl Default for SymbolEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SymbolEngine {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cache: SymbolCache::new(),
        }
    }

    /// Busca símbolos de acordo com as opções informadas.
    pub fn search(&self, options: &SymOptions) -> Vec<Symbol> {
        let mut results = Vec::new();
        let target = &options.path;

        if target.is_file() {
            if let Ok(content) = std::fs::read_to_string(target) {
                let mtime = std::fs::metadata(target)
                    .and_then(|m| m.modified())
                    .unwrap_or(SystemTime::UNIX_EPOCH);

                let symbols = self.cache.get_or_parse(target, &content, mtime);
                for sym in symbols {
                    if matches_filter(&sym, options) {
                        results.push(sym);
                        if results.len() >= options.limit {
                            break;
                        }
                    }
                }
            }
            return results;
        }

        // Varredura de diretório
        let mut builder = WalkBuilder::new(target);
        builder.hidden(!options.all);
        builder.git_ignore(!options.all);
        builder.git_global(!options.all);
        builder.git_exclude(!options.all);

        let query_name_lower = options.name.as_ref().map(|n| n.to_lowercase());

        for result in builder.build() {
            let Ok(entry) = result else {
                continue;
            };

            if entry.file_type().is_none_or(|ft| !ft.is_file()) {
                continue;
            }

            let path = entry.path();
            let Some(lang) = SupportedLanguage::from_path(path) else {
                continue;
            };

            if let Some(ref l) = options.lang {
                if !lang.name().eq_ignore_ascii_case(l) {
                    continue;
                }
            }

            // Pré-filtro rápido: se houver nome de símbolo específico, verifica se a palavra ocorre no buffer
            let Ok(content) = std::fs::read_to_string(path) else {
                continue;
            };

            if let Some(ref q) = query_name_lower {
                if !content.to_lowercase().contains(q) {
                    continue;
                }
            }

            let mtime = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);

            let symbols = self.cache.get_or_parse(path, &content, mtime);
            for sym in symbols {
                if matches_filter(&sym, options) {
                    results.push(sym);
                    if results.len() >= options.limit {
                        return results;
                    }
                }
            }
        }

        // Ordenação por relevância: correspondência exata primeiro, depois por ordem alfabética e caminho
        if let Some(ref target_name) = options.name {
            results.sort_by(|a, b| {
                let a_exact = a.name.eq_ignore_ascii_case(target_name);
                let b_exact = b.name.eq_ignore_ascii_case(target_name);
                match (a_exact, b_exact) {
                    (true, false) => std::cmp::Ordering::Less,
                    (false, true) => std::cmp::Ordering::Greater,
                    _ => a.path.cmp(&b.path).then_with(|| a.line.cmp(&b.line)),
                }
            });
        }

        results
    }
}

fn matches_filter(sym: &Symbol, options: &SymOptions) -> bool {
    if let Some(ref target_name) = options.name {
        if !sym
            .name
            .to_lowercase()
            .contains(&target_name.to_lowercase())
        {
            return false;
        }
    }

    if let Some(kind) = options.kind {
        if sym.kind != kind {
            return false;
        }
    }

    if let Some(ref lang) = options.lang {
        if !sym.language.eq_ignore_ascii_case(lang) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols() {
        let code = r#"
pub struct UserState {
    pub id: u64,
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Process {
    fn execute(&self);
}

pub fn start_worker() -> bool {
    true
}
"#;
        let symbols = extract_symbols(Path::new("dummy.rs"), code);
        assert_eq!(symbols.len(), 5);

        assert!(symbols
            .iter()
            .any(|s| s.name == "UserState" && s.kind == SymbolKind::Struct));
        assert!(symbols
            .iter()
            .any(|s| s.name == "Status" && s.kind == SymbolKind::Enum));
        assert!(symbols
            .iter()
            .any(|s| s.name == "Process" && s.kind == SymbolKind::Trait));
        assert!(symbols
            .iter()
            .any(|s| s.name == "start_worker" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_extract_python_symbols() {
        let code = r#"
class DataModel:
    def process_data(self):
        pass

def global_handler():
    return 42
"#;
        let symbols = extract_symbols(Path::new("script.py"), code);
        assert!(symbols
            .iter()
            .any(|s| s.name == "DataModel" && s.kind == SymbolKind::Class));
        assert!(symbols
            .iter()
            .any(|s| s.name == "process_data" && s.kind == SymbolKind::Function));
        assert!(symbols
            .iter()
            .any(|s| s.name == "global_handler" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_extract_typescript_symbols() {
        let code = r#"
export interface Config {
    port: number;
}

export type StringOrNumber = string | number;

export class ServiceClient {
    connect(): void {}
}

export function createService(): ServiceClient {
    return new ServiceClient();
}
"#;
        let symbols = extract_symbols(Path::new("service.ts"), code);
        assert!(symbols
            .iter()
            .any(|s| s.name == "Config" && s.kind == SymbolKind::Interface));
        assert!(symbols
            .iter()
            .any(|s| s.name == "StringOrNumber" && s.kind == SymbolKind::TypeAlias));
        assert!(symbols
            .iter()
            .any(|s| s.name == "ServiceClient" && s.kind == SymbolKind::Class));
        assert!(symbols
            .iter()
            .any(|s| s.name == "createService" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_extract_go_symbols() {
        let code = r#"
package main

type Config struct {
    Host string
}

type Reader interface {
    Read() []byte
}

func ProcessData() {
}
"#;
        let symbols = extract_symbols(Path::new("main.go"), code);
        assert!(symbols
            .iter()
            .any(|s| s.name == "Config" && s.kind == SymbolKind::Struct));
        assert!(symbols
            .iter()
            .any(|s| s.name == "Reader" && s.kind == SymbolKind::Interface));
        assert!(symbols
            .iter()
            .any(|s| s.name == "ProcessData" && s.kind == SymbolKind::Function));
    }

    #[test]
    fn test_extract_csharp_symbols() {
        let code = r#"
namespace Core {
    public class Worker {
        public void Run() {}
    }

    public interface IHandler {}
}
"#;
        let symbols = extract_symbols(Path::new("Worker.cs"), code);
        assert!(symbols
            .iter()
            .any(|s| s.name == "Worker" && s.kind == SymbolKind::Class));
        assert!(symbols
            .iter()
            .any(|s| s.name == "IHandler" && s.kind == SymbolKind::Interface));
    }

    #[test]
    fn test_symbol_search_with_cache() {
        let engine = SymbolEngine::new();
        let options = SymOptions {
            name: Some("extract_symbols".into()),
            path: PathBuf::from("src"),
            ..Default::default()
        };
        let found = engine.search(&options);
        assert!(!found.is_empty());
        assert_eq!(found[0].name, "extract_symbols");
    }
}

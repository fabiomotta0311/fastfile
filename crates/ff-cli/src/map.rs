//! Gerador de visão compacta de repositórios (`fastfile map` / `ff map`, §3.5 e #26).

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::schema::{LanguageStat, MapFileEntry, MapTreeNode, ModuleSummary, RepoMapResult};
use crate::secrets::is_secret_file;
use crate::util::format_bytes;

/// Configurações para a geração do mapa do repositório.
#[derive(Debug, Clone)]
pub struct MapOptions {
    pub root: PathBuf,
    pub depth: usize,
    pub budget: Option<usize>,
    pub all: bool,
    pub include_secrets: bool,
}

impl Default for MapOptions {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
            depth: 3,
            budget: None,
            all: false,
            include_secrets: false,
        }
    }
}

/// Identifica a linguagem a partir da extensão do arquivo.
#[must_use]
pub fn detect_language(ext: &str) -> Option<&'static str> {
    match ext.to_lowercase().as_str() {
        "rs" => Some("Rust"),
        "ts" | "mts" | "cts" => Some("TypeScript"),
        "tsx" => Some("TSX"),
        "js" | "mjs" | "cjs" | "jsx" => Some("JavaScript"),
        "py" | "pyi" => Some("Python"),
        "go" => Some("Go"),
        "cs" => Some("C#"),
        "toml" => Some("TOML"),
        "json" => Some("JSON"),
        "md" | "markdown" => Some("Markdown"),
        "yaml" | "yml" => Some("YAML"),
        "c" | "h" => Some("C"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("C++"),
        "java" => Some("Java"),
        "kt" | "kts" => Some("Kotlin"),
        "html" | "htm" => Some("HTML"),
        "css" | "scss" | "sass" | "less" => Some("CSS"),
        "sh" | "bash" | "zsh" => Some("Shell"),
        "ps1" | "psm1" => Some("PowerShell"),
        "sql" => Some("SQL"),
        _ => None,
    }
}

/// Verifica se um diretório é lixo de build ou cache conhecido.
#[must_use]
pub fn is_build_junk_dir(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "target"
            | "node_modules"
            | ".git"
            | "dist"
            | "build"
            | ".next"
            | "__pycache__"
            | ".venv"
            | "venv"
            | ".cargo"
            | "vendor"
            | ".idea"
            | ".vscode"
            | "bin"
            | "obj"
    )
}

/// Verifica se o arquivo é considerado um ponto de entrada ou arquivo primário de código.
fn is_primary_code_filename(name: &str) -> bool {
    let lower = name.to_lowercase();
    matches!(
        lower.as_str(),
        "lib.rs"
            | "main.rs"
            | "mod.rs"
            | "index.ts"
            | "index.js"
            | "app.ts"
            | "app.js"
            | "main.ts"
            | "main.js"
            | "main.py"
            | "app.py"
            | "__init__.py"
            | "main.go"
            | "program.cs"
            | "startup.cs"
    )
}

/// Extrai os principais símbolos de um arquivo usando `ff-sym`.
/// No `ff/2`, caminhos do mapa abrem direto da pasta atual (`PATH-CWD-001`); no `ff/1`, são
/// relativos à raiz do repositório.
fn map_out_path(root: &Path, rel: &str) -> String {
    if crate::schema::is_v2() {
        let full = if rel.is_empty() || rel == "." {
            root.to_path_buf()
        } else {
            root.join(rel)
        };
        crate::util::display_path(&full.to_string_lossy())
    } else {
        rel.to_string()
    }
}

fn extract_primary_symbols(path: &Path, content: &str, limit: usize) -> Vec<String> {
    if limit == 0 {
        return Vec::new();
    }

    let symbols = ff_sym::extract_symbols(path, content);
    if symbols.is_empty() {
        return Vec::new();
    }

    // Priorizar tipos/estruturas/classes, depois funções
    let mut prioritized: Vec<_> = symbols
        .into_iter()
        .filter(|s| {
            matches!(
                s.kind,
                ff_sym::SymbolKind::Struct
                    | ff_sym::SymbolKind::Class
                    | ff_sym::SymbolKind::Trait
                    | ff_sym::SymbolKind::Interface
                    | ff_sym::SymbolKind::Enum
                    | ff_sym::SymbolKind::TypeAlias
                    | ff_sym::SymbolKind::Function
            )
        })
        .collect();

    prioritized.sort_by_key(|s| match s.kind {
        ff_sym::SymbolKind::Struct
        | ff_sym::SymbolKind::Class
        | ff_sym::SymbolKind::Trait
        | ff_sym::SymbolKind::Interface
        | ff_sym::SymbolKind::Enum => 1,
        ff_sym::SymbolKind::TypeAlias => 2,
        ff_sym::SymbolKind::Function => 3,
        _ => 4,
    });

    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for sym in prioritized {
        if seen.insert(sym.name.clone()) {
            let label = match sym.kind {
                ff_sym::SymbolKind::Function => format!("{} (fn)", sym.name),
                ff_sym::SymbolKind::Struct => format!("{} (struct)", sym.name),
                ff_sym::SymbolKind::Class => format!("{} (class)", sym.name),
                ff_sym::SymbolKind::Trait => format!("{} (trait)", sym.name),
                ff_sym::SymbolKind::Interface => format!("{} (interface)", sym.name),
                ff_sym::SymbolKind::Enum => format!("{} (enum)", sym.name),
                ff_sym::SymbolKind::TypeAlias => format!("{} (type)", sym.name),
                _ => sym.name,
            };
            result.push(label);
            if result.len() >= limit {
                break;
            }
        }
    }

    result
}

/// Estrutura auxiliar interna para agregar informações de pastas e arquivos.
struct ScannedFileInfo {
    rel_path: String,
    filename: String,
    size: u64,
    ext: Option<String>,
    full_path: PathBuf,
}

/// Gera o resumo compacto do repositório respeitando o orçamento de tokens.
pub fn generate_repo_map(options: &MapOptions) -> Result<(RepoMapResult, bool, usize), String> {
    let canonical_root = options.root.canonicalize().map_err(|e| {
        format!(
            "não foi possível acessar diretório {}: {e}",
            options.root.display()
        )
    })?;

    let mut builder = WalkBuilder::new(&canonical_root);
    builder
        .standard_filters(!options.all)
        .hidden(!options.all)
        .parents(!options.all);

    let walker = builder.build();

    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    let mut total_bytes = 0u64;

    let mut lang_bytes: HashMap<String, u64> = HashMap::new();
    let mut lang_files: HashMap<String, usize> = HashMap::new();

    let mut files_by_dir: BTreeMap<String, Vec<ScannedFileInfo>> = BTreeMap::new();
    let mut dir_sizes: HashMap<String, (u64, usize)> = HashMap::new(); // (bytes, files)

    for result in walker {
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if path == canonical_root {
            continue;
        }

        let rel_path_buf = match path.strip_prefix(&canonical_root) {
            Ok(p) => p.to_path_buf(),
            Err(_) => continue,
        };
        let rel_path = rel_path_buf.to_string_lossy().replace('\\', "/");

        // Checar pastas ignoradas se não foi pedido --all
        if !options.all {
            let has_junk_component = rel_path_buf
                .components()
                .any(|c| is_build_junk_dir(&c.as_os_str().to_string_lossy()));
            if has_junk_component {
                continue;
            }
        }

        // Checar segredos
        if !options.include_secrets {
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if is_secret_file(fname) {
                continue;
            }
        }

        let file_type = entry.file_type();
        if let Some(ft) = file_type {
            if ft.is_dir() {
                total_dirs += 1;
            } else if ft.is_file() {
                total_files += 1;
                let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
                total_bytes += size;

                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                let ext = path
                    .extension()
                    .map(|e| e.to_string_lossy().to_string().to_lowercase());

                if let Some(ref ext_str) = ext {
                    if let Some(lang) = detect_language(ext_str) {
                        *lang_bytes.entry(lang.to_string()).or_insert(0) += size;
                        *lang_files.entry(lang.to_string()).or_insert(0) += 1;
                    }
                }

                let parent_dir = rel_path_buf
                    .parent()
                    .map(|p| p.to_string_lossy().replace('\\', "/"))
                    .unwrap_or_default();

                let (cur_bytes, cur_count) = dir_sizes.entry(parent_dir.clone()).or_insert((0, 0));
                *cur_bytes += size;
                *cur_count += 1;

                files_by_dir
                    .entry(parent_dir)
                    .or_default()
                    .push(ScannedFileInfo {
                        rel_path,
                        filename,
                        size,
                        ext,
                        full_path: path.to_path_buf(),
                    });
            }
        }
    }

    // Estatísticas de linguagem ordenadas por bytes decrescente
    let mut languages: Vec<LanguageStat> = lang_bytes
        .into_iter()
        .map(|(lang, bytes)| {
            let count = lang_files.get(&lang).copied().unwrap_or(0);
            let percentage = if total_bytes > 0 {
                (bytes as f64 / total_bytes as f64) * 100.0
            } else {
                0.0
            };
            LanguageStat {
                language: lang,
                files: count,
                bytes,
                percentage: (percentage * 10.0).round() / 10.0,
            }
        })
        .collect();
    languages.sort_by_key(|a| std::cmp::Reverse(a.bytes));

    // Identificação de módulos e projetos
    // Um módulo pode ser:
    // 1. Cada diretório que contém Cargo.toml, package.json, pyproject.toml ou go.mod
    // 2. Ou subpastas de 1º e 2º nível sob crates/, packages/, src/ ou na raiz.
    let mut module_dirs: Vec<String> = Vec::new();
    for (dir, files) in &files_by_dir {
        let has_manifest = files.iter().any(|f| {
            matches!(
                f.filename.as_str(),
                "Cargo.toml" | "package.json" | "pyproject.toml" | "go.mod" | "*.csproj"
            )
        });
        if has_manifest && !dir.is_empty() {
            module_dirs.push(dir.clone());
        }
    }

    // Se nenhum manifesto foi achado em subpastas, usar diretórios de nível 1 ou 2
    if module_dirs.is_empty() {
        for dir in files_by_dir.keys() {
            if !dir.is_empty() {
                let depth = dir.split('/').count();
                if depth <= 2 && !module_dirs.contains(dir) {
                    module_dirs.push(dir.clone());
                }
            }
        }
    }

    // Caso raiz ainda não esteja coberta e tenha arquivos
    if files_by_dir.contains_key("") && !module_dirs.iter().any(|d| d.is_empty()) {
        module_dirs.push(".".to_string());
    }

    // Mapear cada módulo
    let mut modules = Vec::new();
    for mod_dir in &module_dirs {
        let normalized = if mod_dir == "." { "" } else { mod_dir.as_str() };
        let mut mod_files_count = 0usize;
        let mut mod_bytes = 0u64;
        let mut primary_candidates: Vec<&ScannedFileInfo> = Vec::new();

        for (dir, files) in &files_by_dir {
            let belongs = if normalized.is_empty() {
                !dir.contains('/')
            } else {
                dir == normalized || dir.starts_with(&format!("{normalized}/"))
            };

            if belongs {
                for file in files {
                    mod_files_count += 1;
                    mod_bytes += file.size;

                    if is_primary_code_filename(&file.filename)
                        || file.ext.as_deref() == Some("rs")
                        || file.ext.as_deref() == Some("ts")
                        || file.ext.as_deref() == Some("py")
                        || file.ext.as_deref() == Some("go")
                        || file.ext.as_deref() == Some("cs")
                    {
                        primary_candidates.push(file);
                    }
                }
            }
        }

        // Ordenar candidatos: primeiro primários explícitos (lib.rs, etc.), depois por tamanho
        primary_candidates.sort_by(|a, b| {
            let a_pri = is_primary_code_filename(&a.filename);
            let b_pri = is_primary_code_filename(&b.filename);
            b_pri.cmp(&a_pri).then_with(|| b.size.cmp(&a_size(&a.size)))
        });

        // Pegar até 5 arquivos primários por módulo
        let mut primary_files = Vec::new();
        for file in primary_candidates.into_iter().take(5) {
            let lang = file.ext.as_deref().and_then(detect_language);
            let content = fs::read_to_string(&file.full_path).unwrap_or_default();
            let symbols = extract_primary_symbols(&file.full_path, &content, 6);

            primary_files.push(MapFileEntry {
                path: map_out_path(&canonical_root, &file.rel_path),
                name: file.filename.clone(),
                size: file.size,
                language: lang.map(|s| s.to_lowercase()),
                symbols,
            });
        }

        let display_name = if mod_dir == "." {
            options
                .root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "root".to_string())
        } else {
            mod_dir.clone()
        };

        modules.push(ModuleSummary {
            path: map_out_path(&canonical_root, mod_dir),
            name: display_name,
            files: mod_files_count,
            bytes: mod_bytes,
            primary_files,
        });
    }

    // Construir árvore compacta até profundidade pedida
    let mut tree = Vec::new();
    let max_depth = options.depth;

    for (dir, files) in &files_by_dir {
        let depth = if dir.is_empty() {
            0
        } else {
            dir.split('/').count()
        };

        if depth <= max_depth {
            if !dir.is_empty() {
                let (d_bytes, d_count) = dir_sizes.get(dir).copied().unwrap_or((0, 0));
                let dir_name = dir.split('/').next_back().unwrap_or(dir);
                tree.push(MapTreeNode {
                    path: map_out_path(&canonical_root, dir),
                    name: dir_name.to_string(),
                    is_dir: true,
                    size: d_bytes,
                    depth,
                    files_count: Some(d_count),
                    symbols: Vec::new(),
                });
            }

            // Adicionar arquivos primários ou relevantes nessa pasta
            for file in files {
                if depth < max_depth {
                    let is_primary =
                        is_primary_code_filename(&file.filename) || file.size > 2048 || depth == 0;
                    if is_primary {
                        let content = fs::read_to_string(&file.full_path).unwrap_or_default();
                        let symbols = extract_primary_symbols(&file.full_path, &content, 4);
                        tree.push(MapTreeNode {
                            path: map_out_path(&canonical_root, &file.rel_path),
                            name: file.filename.clone(),
                            is_dir: false,
                            size: file.size,
                            depth: depth + 1,
                            files_count: None,
                            symbols,
                        });
                    }
                }
            }
        }
    }

    let root_str = if crate::schema::is_v2() {
        crate::util::display_path(&canonical_root.to_string_lossy())
    } else {
        canonical_root.to_string_lossy().replace('\\', "/")
    };

    let mut result = RepoMapResult {
        root: root_str,
        total_files,
        total_dirs,
        total_bytes,
        languages,
        modules,
        tree,
    };

    // Compactação por orçamento de tokens
    let mut truncated = false;
    let mut used_tokens = estimate_tokens(&result);

    if let Some(budget) = options.budget {
        if used_tokens > budget {
            truncated = true;

            // Nível 1: Reduzir símbolos para no máximo 2 por arquivo
            for m in &mut result.modules {
                for pf in &mut m.primary_files {
                    pf.symbols.truncate(2);
                }
            }
            for node in &mut result.tree {
                node.symbols.truncate(2);
            }
            used_tokens = estimate_tokens(&result);

            // Nível 2: Se ainda estourar, limpar árvore detalhada e focar nos módulos
            if used_tokens > budget {
                result.tree.clear();
                used_tokens = estimate_tokens(&result);
            }

            // Nível 3: Reduzir símbolos para 1 por arquivo nos módulos
            if used_tokens > budget {
                for m in &mut result.modules {
                    for pf in &mut m.primary_files {
                        pf.symbols.truncate(1);
                    }
                    m.primary_files.truncate(2);
                }
                used_tokens = estimate_tokens(&result);
            }

            // Nível 4: Reduzir linguagens para top 5
            if used_tokens > budget {
                result.languages.truncate(5);
                used_tokens = estimate_tokens(&result);
            }
        }
    }

    Ok((result, truncated, used_tokens))
}

fn a_size(s: &u64) -> u64 {
    *s
}

/// Estima o número aproximado de tokens que a estrutura serializada consome.
pub fn estimate_tokens<T: serde::Serialize>(val: &T) -> usize {
    match serde_json::to_string(val) {
        Ok(json_str) => json_str.len().div_ceil(4),
        Err(_) => 0,
    }
}

/// Formata a visão do repositório para exibição humana compacta no terminal.
#[must_use]
pub fn format_repo_map_human(map: &RepoMapResult, truncated: bool) -> String {
    let mut out = String::new();
    out.push_str(&format!("FastFile Repository Map: {}\n", map.root));
    out.push_str(&format!(
        "Total: {} arquivos ({}) em {} diretórios\n\n",
        map.total_files,
        format_bytes(map.total_bytes),
        map.total_dirs
    ));

    out.push_str("Linguagens:\n");
    for lang in &map.languages {
        out.push_str(&format!(
            "  {:<14} {:>4} arquivos ({:>5.1}%) {:>10}\n",
            lang.language,
            lang.files,
            lang.percentage,
            format_bytes(lang.bytes)
        ));
    }

    if !map.modules.is_empty() {
        out.push_str("\nMódulos & Componentes Principais:\n");
        for m in &map.modules {
            out.push_str(&format!(
                "  📁 {} ({} arquivos, {})\n",
                m.name,
                m.files,
                format_bytes(m.bytes)
            ));
            for pf in &m.primary_files {
                let lang_str = pf.language.as_deref().unwrap_or("code");
                out.push_str(&format!(
                    "     📄 {} [{}, {}]\n",
                    pf.path,
                    lang_str,
                    format_bytes(pf.size)
                ));
                if !pf.symbols.is_empty() {
                    out.push_str(&format!("        ↳ {}\n", pf.symbols.join(", ")));
                }
            }
        }
    }

    if !map.tree.is_empty() {
        out.push_str("\nEstrutura de Pastas:\n");
        for node in &map.tree {
            let indent = "  ".repeat(node.depth);
            if node.is_dir {
                out.push_str(&format!(
                    "{}📁 {}/ ({})\n",
                    indent,
                    node.name,
                    format_bytes(node.size)
                ));
            } else {
                out.push_str(&format!(
                    "{}📄 {} ({})\n",
                    indent,
                    node.name,
                    format_bytes(node.size)
                ));
                if !node.symbols.is_empty() {
                    out.push_str(&format!("{}   ↳ {}\n", indent, node.symbols.join(", ")));
                }
            }
        }
    }

    if truncated {
        out.push_str("\n⚠️ Saída compactada para respeitar o orçamento de tokens (--budget).\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language("rs"), Some("Rust"));
        assert_eq!(detect_language("ts"), Some("TypeScript"));
        assert_eq!(detect_language("tsx"), Some("TSX"));
        assert_eq!(detect_language("py"), Some("Python"));
        assert_eq!(detect_language("go"), Some("Go"));
        assert_eq!(detect_language("cs"), Some("C#"));
        assert_eq!(detect_language("toml"), Some("TOML"));
        assert_eq!(detect_language("unknown"), None);
    }

    #[test]
    fn test_is_build_junk_dir() {
        assert!(is_build_junk_dir("target"));
        assert!(is_build_junk_dir("node_modules"));
        assert!(is_build_junk_dir(".git"));
        assert!(is_build_junk_dir("dist"));
        assert!(is_build_junk_dir("build"));
        assert!(is_build_junk_dir(".venv"));
        assert!(!is_build_junk_dir("src"));
        assert!(!is_build_junk_dir("crates"));
    }

    #[test]
    fn test_generate_repo_map_current_dir() {
        let options = MapOptions {
            root: PathBuf::from("."),
            depth: 2,
            budget: Some(1500),
            all: false,
            include_secrets: false,
        };

        let result = generate_repo_map(&options);
        assert!(
            result.is_ok(),
            "generate_repo_map falhou: {:?}",
            result.err()
        );
        let (map, _truncated, tokens) = result.unwrap();

        assert!(map.total_files > 0);
        assert!(map.total_bytes > 0);
        assert!(!map.languages.is_empty());
        assert!(map.languages.iter().any(|l| l.language == "Rust"));
        assert!(tokens > 0);

        let formatted = format_repo_map_human(&map, false);
        assert!(formatted.contains("FastFile Repository Map"));
        assert!(formatted.contains("Rust"));
    }
}

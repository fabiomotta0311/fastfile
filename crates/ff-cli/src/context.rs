//! Protótipo CLI de contexto Rust. Evidência do snapshot local, não resolução semântica.

use std::collections::BTreeSet;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Args;
use ff_sym::RustSite;
use serde::Serialize;
use serde_json::{json, Value};

use crate::secrets::{is_secret_file, mask_secrets};
use crate::{read, util::display_path_from, CliError};

const MAX_ENTRIES: usize = 20_000;
const MAX_FILES: usize = 512;
const MAX_FILE_BYTES: u64 = 256 * 1024;
const MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_MATCH_LINES: usize = 128;
const MAX_EVIDENCE: usize = 32;
const AROUND: usize = 2;

#[derive(Args, Debug, PartialEq, Eq)]
pub struct ContextArgs {
    /// Diretório autorizado para a busca
    #[arg(long)]
    root: PathBuf,
    /// Caminho relativo à pasta atual ou absoluto, com linha de base 1
    #[arg(long)]
    location: Option<String>,
    /// Identificador Rust sem qualificação
    #[arg(long)]
    symbol: Option<String>,
    /// Diagnóstico delimitado: caminho:linha|mensagem (mensagem não devolvida)
    #[arg(long)]
    diagnostic: Option<String>,
}

struct Source {
    display: String,
    text: String,
    sites: Vec<RustSite>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Location,
    Definition,
    Test,
    TextualReference,
}

impl Kind {
    fn status(self) -> &'static str {
        match self {
            Self::Location | Self::Definition => "observed",
            Self::Test | Self::TextualReference => "candidate",
        }
    }
}

#[derive(Serialize, PartialEq, Eq, PartialOrd, Ord)]
struct Evidence {
    kind: Kind,
    line: usize,
    reasons: Vec<&'static str>,
}

struct Range {
    source: usize,
    start: usize,
    end: usize,
    evidence: Vec<Evidence>,
}

fn bad(message: &str) -> CliError {
    CliError::InvalidArgument(message.to_owned())
}

fn excluded_name(name: &str) -> bool {
    name.starts_with('.') || is_secret_file(name) || ff_index::is_ignored_build_dir(name)
}

fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT, incluindo junctions.
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    meta.file_type().is_symlink()
}

fn safe_ignore_files(dir: &Path) -> bool {
    [".ignore", ".gitignore"]
        .into_iter()
        .all(|name| match fs::symlink_metadata(dir.join(name)) {
            Ok(meta) => meta.is_file() && !is_link(&meta) && meta.len() <= MAX_FILE_BYTES,
            Err(err) => err.kind() == std::io::ErrorKind::NotFound,
        })
}

fn load_source(path: &Path, root: &Path, cwd: &Path) -> Result<Source, &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "unreadable_source")?;
    if !meta.is_file() || is_link(&meta) {
        return Err("unreadable_source");
    }
    let real = path.canonicalize().map_err(|_| "unreadable_source")?;
    if !real.starts_with(root) || real != path {
        return Err("scope_changed");
    }
    let display = real
        .to_str()
        .filter(|path| !path.chars().any(char::is_control))
        .filter(|path| cfg!(windows) || !path.contains('\\'))
        .map(|path| display_path_from(path, Some(cwd)))
        .filter(|path| mask_secrets(path).1 == 0)
        .ok_or("restricted_path")?;
    if meta.len() > MAX_FILE_BYTES {
        return Err("file_size_limit");
    }
    let mut bytes = Vec::new();
    File::open(&real)
        .and_then(|file| file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|_| "unreadable_source")?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err("file_size_limit");
    }
    if bytes.contains(&0) {
        return Err("unsupported_encoding");
    }
    let text = String::from_utf8(bytes).map_err(|_| "unsupported_encoding")?;
    let text = text
        .strip_prefix('\u{feff}')
        .map(str::to_owned)
        .unwrap_or(text);
    Ok(Source {
        display,
        text,
        sites: Vec::new(),
    })
}

fn collect(root: &Path, cwd: &Path, warnings: &mut BTreeSet<&'static str>) -> Vec<Source> {
    let skipped_directory = Arc::new(AtomicBool::new(false));
    let skipped = Arc::clone(&skipped_directory);
    let walker = ignore::WalkBuilder::new(root)
        // Não consulta regras globais, ancestrais ou gitdirs de worktrees fora da raiz.
        .parents(false)
        .git_global(false)
        .git_exclude(false)
        .require_git(false)
        .follow_links(false)
        .sort_by_file_path(Path::cmp)
        .filter_entry(move |entry| {
            let Some(meta) = fs::symlink_metadata(entry.path()).ok() else {
                skipped.store(true, Ordering::Relaxed);
                return false;
            };
            if is_link(&meta) {
                return false;
            }
            if entry.depth() != 0 && entry.file_name().to_str().is_none_or(excluded_name) {
                return false;
            }
            if meta.is_dir() && !safe_ignore_files(entry.path()) {
                skipped.store(true, Ordering::Relaxed);
                return false;
            }
            true
        })
        .build();
    let mut sources = Vec::new();
    let mut bytes = 0;
    let mut files = 0;
    for (index, entry) in walker.enumerate() {
        if index == MAX_ENTRIES {
            warnings.insert("entry_limit");
            break;
        }
        let Ok(entry) = entry else {
            warnings.insert("walk_error");
            continue;
        };
        // Regras não confiáveis podem comprometer a seleção; falhar fechado.
        if entry.error().is_some() {
            warnings.insert("ignore_error");
            return Vec::new();
        }
        let path = entry.path();
        if !entry.file_type().is_some_and(|ft| ft.is_file())
            || path
                .extension()
                .is_none_or(|e| !e.eq_ignore_ascii_case("rs"))
        {
            continue;
        }
        files += 1;
        if files > MAX_FILES {
            warnings.insert("file_limit");
            break;
        }
        match load_source(path, root, cwd) {
            Ok(source) => {
                bytes += source.text.len();
                if bytes > MAX_TOTAL_BYTES {
                    warnings.insert("scan_byte_limit");
                    break;
                }
                sources.push(source);
            }
            Err(code) => {
                warnings.insert(code);
            }
        }
    }
    if skipped_directory.load(Ordering::Relaxed) {
        warnings.insert("unreadable_entry_or_ignore_file");
    }
    sources.sort_by(|a, b| a.display.cmp(&b.display));
    sources
}

fn location(raw: &str) -> Result<(&str, usize), CliError> {
    if raw.len() > 4096 || raw.contains(['\r', '\n', '\0']) {
        return Err(bad("localização inválida"));
    }
    let value = read::parse_spec(raw, 0);
    let Some(line) = value.focus else {
        return Err(bad("use caminho:linha (linha positiva)"));
    };
    let path = &raw[..value.path.len()];
    if Path::new(path)
        .components()
        .any(|c| c == std::path::Component::ParentDir)
        || (!cfg!(windows) && path.contains('\\'))
    {
        return Err(bad("use um caminho direto, sem componentes '..'"));
    }
    Ok((path, line))
}

fn select(
    sources: &[Source],
    observed: Option<(usize, usize)>,
    symbol: Option<&str>,
    warnings: &mut BTreeSet<&'static str>,
) -> (Vec<Range>, usize) {
    let mut definitions = 0;
    let mut ranges = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let matches: Vec<_> = source
            .sites
            .iter()
            .filter(|s| symbol == Some(s.name.as_str()))
            .collect();
        definitions += matches.len();
        let anchor_line = observed
            .filter(|(file, _)| *file == index)
            .map(|(_, line)| line);
        let mut hits = Vec::new();
        let mut add = |kind, line, reasons| {
            hits.push(Evidence {
                kind,
                line,
                reasons,
            })
        };
        if let Some(line) = anchor_line {
            add(Kind::Location, line, vec!["input_location"]);
        }
        for site in &matches {
            add(Kind::Definition, site.start, vec!["syntactic_definition"]);
        }
        let mut refs = symbol
            .map_or_else(
                || Ok(Vec::new()),
                |word| ff_content::literal_line_numbers(&source.text, word, MAX_MATCH_LINES + 1),
            )
            .unwrap_or_else(|_| {
                warnings.insert("grep_error");
                Vec::new()
            });
        if refs.len() > MAX_MATCH_LINES {
            warnings.insert("reference_limit");
            refs.truncate(MAX_MATCH_LINES);
        }
        refs.retain(|line| !matches.iter().any(|s| s.start == *line));
        for site in source.sites.iter().filter(|site| site.is_test) {
            let mut reasons = Vec::new();
            if refs
                .iter()
                .any(|line| (site.start..=site.end).contains(line))
            {
                reasons.push("textual_reference");
            }
            if matches
                .iter()
                .any(|s| s.module == site.module && s.start != site.start)
            {
                reasons.push("same_module");
            }
            if anchor_line.is_some_and(|line| line.abs_diff(site.start) <= 24)
                || matches.iter().any(|s| s.start.abs_diff(site.start) <= 24)
            {
                reasons.push("adjacent_test");
            }
            if !reasons.is_empty() {
                add(Kind::Test, site.start, reasons);
            }
        }
        for line in refs {
            add(Kind::TextualReference, line, vec!["literal_identifier"]);
        }
        ranges.extend(hits.into_iter().map(|hit| (index, hit)));
    }
    ranges.sort_unstable_by(|(ai, a), (bi, b)| {
        (a.kind, ai, a.line, &a.reasons).cmp(&(b.kind, bi, b.line, &b.reasons))
    });
    ranges.dedup();
    if ranges.len() > MAX_EVIDENCE {
        warnings.insert("evidence_limit");
        ranges.truncate(MAX_EVIDENCE);
    }
    ranges.sort_unstable_by_key(|(index, hit)| (*index, hit.line, hit.kind));
    let mut merged: Vec<Range> = Vec::new();
    for (index, hit) in ranges {
        let start = hit.line.saturating_sub(AROUND).max(1);
        let end = hit.line + AROUND;
        if let Some(prev) = merged
            .last_mut()
            .filter(|r| r.source == index && start <= r.end)
        {
            prev.end = prev.end.max(end);
            prev.evidence.push(hit);
        } else {
            merged.push(Range {
                source: index,
                start,
                end,
                evidence: vec![hit],
            });
        }
    }
    (merged, definitions)
}

pub(crate) fn execute(args: &ContextArgs) -> Result<Value, CliError> {
    if [
        args.location.is_some(),
        args.symbol.is_some(),
        args.diagnostic.is_some(),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
        != 1
    {
        return Err(bad(
            "forneça exatamente um de --location, --symbol ou --diagnostic",
        ));
    }
    let (input, at) = if let Some(raw) = args.diagnostic.as_deref() {
        let (where_, message) = raw
            .split_once('|')
            .ok_or_else(|| bad("use caminho:linha|mensagem"))?;
        if raw.len() > 8192 || message.trim().is_empty() {
            return Err(bad("diagnóstico vazio ou acima do limite"));
        }
        ("diagnostic", Some(location(where_)?))
    } else if let Some(raw) = args.location.as_deref() {
        ("location", Some(location(raw)?))
    } else {
        ("symbol", None)
    };
    if let Some(word) = args.symbol.as_deref() {
        let ident = regex::Regex::new(r"^(?:r#)?[_\p{XID_Start}]\p{XID_Continue}*$")
            .expect("regex de identificador válida");
        if word.len() > 128 || !ident.is_match(word) || mask_secrets(word).1 != 0 {
            return Err(bad("use um identificador Rust simples e não sensível"));
        }
    }
    let cwd =
        std::env::current_dir().map_err(|_| CliError::Io("pasta atual indisponível".into()))?;
    #[cfg(windows)]
    if matches!(args.root.components().next(), Some(std::path::Component::Prefix(p)) if !matches!(p.kind(), std::path::Prefix::VerbatimDisk(_) | std::path::Prefix::Disk(_)))
    {
        return Err(bad(
            "use uma letra de unidade, não um caminho UNC ou de dispositivo",
        ));
    }
    if fs::symlink_metadata(&args.root).is_ok_and(|meta| is_link(&meta)) {
        return Err(bad("a raiz não pode ser um link"));
    }
    let root = args
        .root
        .canonicalize()
        .map_err(|_| bad("raiz inacessível"))?;
    if !root.is_dir()
        || root
            .components()
            .any(|c| c.as_os_str().to_str().is_none_or(excluded_name))
    {
        return Err(bad("raiz inválida ou restrita"));
    }
    #[cfg(windows)]
    if !matches!(root.components().next(), Some(std::path::Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::VerbatimDisk(_) | std::path::Prefix::Disk(_)))
    {
        return Err(bad("a raiz deve estar em um volume local"));
    }
    let mut warnings = BTreeSet::new();
    let mut sources = collect(&root, &cwd, &mut warnings);
    for source in &mut sources {
        match ff_sym::rust_sites(&source.text) {
            Some((sites, errors)) => {
                source.sites = sites;
                if errors {
                    warnings.insert("syntax_errors");
                }
            }
            None => {
                warnings.insert("parse_error");
            }
        }
    }
    let observed = at.and_then(|(path, line)| {
        let display = display_path_from(path, Some(&cwd));
        let mut matching = sources.iter().enumerate().filter(|(_, s)| {
            if cfg!(windows) {
                s.display.eq_ignore_ascii_case(&display)
            } else {
                s.display == display
            }
        });
        let (index, source) = matching.next()?;
        (matching.next().is_none() && line <= source.text.lines().count()).then_some((index, line))
    });
    let anchor_site = observed.and_then(|(index, line)| {
        sources[index]
            .sites
            .iter()
            .filter(|site| (site.start..=site.end).contains(&line))
            .min_by_key(|site| (site.end - site.start, std::cmp::Reverse(site.start)))
    });
    let symbol = args
        .symbol
        .as_deref()
        .or_else(|| anchor_site.map(|site| site.name.as_str()))
        .filter(|name| mask_secrets(name).1 == 0 && name.len() <= 128);
    let (ranges, definitions) = select(&sources, observed, symbol, &mut warnings);
    let mut snippets = Vec::new();
    for range in ranges {
        let source = &sources[range.source];
        let mut end = range.start - 1;
        let mut content = String::new();
        for line in source.text.lines().skip(end).take(range.end - end) {
            let (line, cut) = read::truncate_line(mask_secrets(line).0);
            if cut {
                warnings.insert("line_truncated");
            }
            content.push_str(&line);
            content.push('\n');
            end += 1;
        }
        let evidence: Vec<_> = range.evidence.into_iter().map(|e| {
            json!({"kind": e.kind, "line": e.line, "status": e.kind.status(), "reasons": e.reasons})
        }).collect();
        snippets.push(json!({"path": source.display, "start_line": range.start, "end_line": end, "content": content, "evidence": evidence}));
    }
    let anchor = match observed {
        Some((index, line)) => {
            json!({"kind": input, "status": "observed", "path": sources[index].display, "line": line})
        }
        None if at.is_some() => json!({"kind": input, "status": "unverified"}),
        None => json!({"kind": input, "status": "query"}),
    };
    let symbol_status = match (symbol, definitions, warnings.is_empty()) {
        (None, _, _) => "not_inferred",
        (Some(_), 2.., _) => "ambiguous",
        (Some(_), _, false) => "partial",
        (Some(_), 1, true) => "one_candidate",
        (Some(_), _, true) => "not_observed",
    };
    Ok(
        json!({"schema": "ff-context/0", "command": "context", "source": "local_rust_snapshot", "anchor": anchor,
        "symbol": symbol, "symbol_status": symbol_status, "definition_count": definitions,
        "coverage": if warnings.is_empty() { "scanned_scope" } else { "partial" },
        "rust_files_read": sources.len(), "warnings": warnings, "snippets": snippets}),
    )
}

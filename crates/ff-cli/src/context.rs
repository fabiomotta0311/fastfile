//! Protótipo CLI de contexto Rust. Evidência do snapshot local, não resolução semântica.

use std::collections::BTreeSet;
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

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
/// Orçamento padrão e teto da resposta, em tokens aproximados (1 token ≈ 4 bytes).
pub const DEFAULT_BUDGET: usize = 4_000;
pub const MAX_BUDGET: usize = 50_000;
const SCHEMA: &str = "ff-context/1";

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
    /// Orçamento aproximado da resposta em tokens (1 a 50000); o excedente vira `omitted`
    #[arg(long, default_value_t = DEFAULT_BUDGET)]
    budget: usize,
    /// Prazo cooperativo em ms; ao expirar a resposta é parcial (0 expira de imediato)
    #[arg(long)]
    deadline_ms: Option<u64>,
}

struct Deadline(Option<Instant>);

impl Deadline {
    fn new(ms: Option<u64>) -> Self {
        Self(ms.and_then(|ms| Instant::now().checked_add(Duration::from_millis(ms))))
    }
    fn expired(&self) -> bool {
        self.0.is_some_and(|at| Instant::now() >= at)
    }
}

struct Source {
    path: PathBuf,
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

fn safe_display(path: &Path, cwd: &Path) -> Option<String> {
    path.to_str()
        .filter(|path| !path.chars().any(char::is_control))
        .filter(|path| cfg!(windows) || !path.contains('\\'))
        .map(|path| display_path_from(path, Some(cwd)))
        .filter(|path| mask_secrets(path).1 == 0)
}

fn stamp(meta: &Metadata) -> (u64, Option<std::time::SystemTime>) {
    (meta.len(), meta.modified().ok())
}

/// Lê um arquivo Rust da raiz. `after_read` roda entre a leitura e a revalidação (ponto de teste);
/// se o arquivo mudou nesse intervalo, ele é descartado e o escopo vira parcial.
fn load_source(
    path: &Path,
    root: &Path,
    cwd: &Path,
    after_read: &dyn Fn(),
) -> Result<Source, &'static str> {
    let meta = fs::symlink_metadata(path).map_err(|_| "unreadable_source")?;
    if !meta.is_file() || is_link(&meta) {
        return Err("unreadable_source");
    }
    let real = path.canonicalize().map_err(|_| "unreadable_source")?;
    if !real.starts_with(root) || real != path {
        return Err("scope_changed");
    }
    let display = safe_display(&real, cwd).ok_or("restricted_path")?;
    if meta.len() > MAX_FILE_BYTES {
        return Err("file_size_limit");
    }
    let file = File::open(&real).map_err(|_| "unreadable_source")?;
    let before = file.metadata().map_err(|_| "unreadable_source")?;
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "unreadable_source")?;
    after_read();
    // Revalida pelo identificador aberto e pelo caminho: troca ou edição durante a leitura invalida.
    let reopened = File::open(&real)
        .and_then(|f| f.metadata())
        .map_err(|_| "file_changed_during_read")?;
    let on_path = fs::symlink_metadata(&real).map_err(|_| "file_changed_during_read")?;
    if is_link(&on_path)
        || real.canonicalize().ok().as_deref() != Some(real.as_path())
        || stamp(&reopened) != stamp(&before)
        || stamp(&on_path) != stamp(&before)
        || bytes.len() as u64 != before.len()
    {
        return Err("file_changed_during_read");
    }
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
        path: real,
        display,
        text,
        sites: Vec::new(),
    })
}

fn collect(
    root: &Path,
    cwd: &Path,
    deadline: &Deadline,
    warnings: &mut BTreeSet<&'static str>,
    skipped_files: &mut usize,
) -> Vec<Source> {
    let skipped_directory = Arc::new(AtomicBool::new(false));
    let skipped = Arc::clone(&skipped_directory);
    let skipped_link = Arc::new(AtomicBool::new(false));
    let link_flag = Arc::clone(&skipped_link);
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
                link_flag.store(true, Ordering::Relaxed);
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
        if deadline.expired() {
            warnings.insert("deadline_exceeded");
            break;
        }
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
        match load_source(path, root, cwd, &|| {}) {
            Ok(source) => {
                bytes += source.text.len();
                if bytes > MAX_TOTAL_BYTES {
                    warnings.insert("scan_byte_limit");
                    break;
                }
                sources.push(source);
            }
            Err(code) => {
                *skipped_files += 1;
                warnings.insert(code);
            }
        }
    }
    if skipped_link.load(Ordering::Relaxed) {
        warnings.insert("link_skipped");
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
    run(args, args.deadline_ms)
}

/// Entrada do MCP: valida os campos (sem modo irrestrito) e limita o prazo cooperativo ao teto
/// informado, que deve ficar abaixo do limite duro da chamada para a resposta parcial chegar.
pub fn run_json(args: &Value, soft_limit: Duration) -> Result<Value, CliError> {
    const KEYS: [&str; 6] = [
        "root",
        "location",
        "symbol",
        "diagnostic",
        "budget",
        "timeout_ms",
    ];
    let obj = args
        .as_object()
        .ok_or_else(|| bad("os argumentos devem ser um objeto"))?;
    if obj.keys().any(|key| !KEYS.contains(&key.as_str())) {
        return Err(bad(
            "campos desconhecidos; consulte o esquema da ferramenta",
        ));
    }
    let text = |key: &str| match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(_) => Err(bad("os campos de texto devem ser strings")),
    };
    let number = |key: &str| match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| bad("budget e timeout_ms devem ser inteiros não negativos")),
    };
    let root = text("root")?.ok_or_else(|| bad("o campo 'root' é obrigatório"))?;
    let budget = number("budget")?.map_or(Ok(DEFAULT_BUDGET), |value| {
        usize::try_from(value).map_err(|_| bad("budget fora do intervalo"))
    })?;
    let soft = u64::try_from(soft_limit.as_millis()).unwrap_or(u64::MAX);
    let deadline_ms = number("timeout_ms")?.map_or(soft, |ms| ms.min(soft));
    let args = ContextArgs {
        root: PathBuf::from(root),
        location: text("location")?,
        symbol: text("symbol")?,
        diagnostic: text("diagnostic")?,
        budget,
        deadline_ms: Some(deadline_ms),
    };
    run(&args, args.deadline_ms)
}

struct Piece {
    priority: Kind,
    json: Value,
}

impl Piece {
    fn omitted(&self) -> Value {
        let path = self.json["path"].as_str().unwrap_or_default();
        let (start, end) = (&self.json["start_line"], &self.json["end_line"]);
        json!({"path": path, "start_line": start, "end_line": end,
            "status": self.priority.status(), "read": format!("{path}:{start}-{end}")})
    }
}

struct Shape<'a> {
    base: &'a Value,
    warnings: &'a BTreeSet<&'static str>,
    pieces: &'a [Piece],
    budget: usize,
}

impl Shape<'_> {
    fn compose(&self, keep: &[bool], list_omitted: bool, used: usize) -> Value {
        let truncated = keep.contains(&false);
        let mut warnings = self.warnings.clone();
        if truncated {
            warnings.insert("budget_truncated");
        }
        let anchor_ok = self.base["anchor"]["status"] != "unverified";
        let omitted: Vec<Value> = (self.pieces.iter().zip(keep))
            .filter(|(_, kept)| !**kept)
            .map(|(piece, _)| piece.omitted())
            .collect();
        let mut value = self.base.clone();
        let object = value.as_object_mut().expect("base é um objeto");
        object.insert(
            "coverage".into(),
            json!(if warnings.is_empty() {
                "scanned_scope"
            } else {
                "partial"
            }),
        );
        // Só um resultado vazio de escopo completo, sem corte e com âncora verificada prova ausência.
        object.insert(
            "empty_means_absent".into(),
            json!(warnings.is_empty() && anchor_ok),
        );
        object.insert("truncated".into(), json!(truncated));
        object.insert("warnings".into(), json!(warnings));
        object.insert(
            "budget".into(),
            json!({"tokens": self.budget, "used_tokens": used, "truncated": truncated}),
        );
        let kept = self.pieces.iter().zip(keep).filter(|(_, kept)| **kept);
        object.insert(
            "snippets".into(),
            Value::Array(kept.map(|(piece, _)| piece.json.clone()).collect()),
        );
        object.insert("omitted_count".into(), json!(omitted.len()));
        if list_omitted && !omitted.is_empty() {
            object.insert("omitted".into(), Value::Array(omitted));
        }
        value
    }

    /// Escolhe, por prioridade (observado, testes, referências), o que cabe em `budget` tokens.
    fn fit(&self) -> Value {
        let limit = self.budget.saturating_mul(4);
        let size = |keep: &[bool], list: bool| self.compose(keep, list, 99_999).to_string().len();
        let mut order: Vec<usize> = (0..self.pieces.len()).collect();
        order.sort_by_key(|&index| (self.pieces[index].priority, index));
        let greedy = |list: bool| {
            let mut keep = vec![false; self.pieces.len()];
            for &index in &order {
                keep[index] = true;
                if size(&keep, list) > limit {
                    keep[index] = false;
                }
            }
            keep
        };
        let count = |keep: &[bool]| keep.iter().filter(|kept| **kept).count();
        // Prefere reservar espaço para listar o que foi omitido (releitura dirigida), mas nunca
        // à custa de perder toda a evidência observada que caberia sem a lista.
        let (with_list, without_list) = (greedy(true), greedy(false));
        let mut keep = if size(&with_list, true) <= limit
            && (count(&with_list) > 0 || count(&without_list) == 0)
        {
            with_list
        } else {
            without_list
        };
        loop {
            for list in [true, false] {
                if size(&keep, list) <= limit {
                    let mut value = self.compose(&keep, list, 0);
                    let used = value.to_string().len().div_ceil(4);
                    value["budget"]["used_tokens"] = json!(used);
                    return value;
                }
            }
            match order.iter().rev().find(|&&index| keep[index]) {
                Some(&index) => keep[index] = false,
                None => break,
            }
        }
        // Nem o envelope cabe: devolve o menor possível, sempre parcial e nunca "ausente".
        json!({"schema": SCHEMA, "command": "context", "coverage": "partial", "truncated": true,
            "empty_means_absent": false, "warnings": ["budget_too_small"]})
    }
}

fn run(args: &ContextArgs, deadline_ms: Option<u64>) -> Result<Value, CliError> {
    if !(1..=MAX_BUDGET).contains(&args.budget) {
        return Err(bad("budget deve ficar entre 1 e 50000 tokens"));
    }
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
    let cwd = cwd.canonicalize().unwrap_or(cwd);
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
    let deadline = Deadline::new(deadline_ms);
    let mut warnings = BTreeSet::new();
    let mut skipped_files = 0;
    let mut sources = collect(&root, &cwd, &deadline, &mut warnings, &mut skipped_files);
    for source in &mut sources {
        if deadline.expired() {
            warnings.insert("deadline_exceeded");
            break;
        }
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
    let (observed, anchor_reason) = match at {
        None => (None, None),
        Some((path, line)) => {
            // Compare canonical paths: Windows may expose short-name or verbatim input spellings,
            // while the walker stores the long canonical path.
            let requested = Path::new(path);
            let requested = if requested.is_absolute() {
                requested.to_path_buf()
            } else {
                cwd.join(requested)
            };
            let requested = requested.canonicalize().ok();
            let found: Vec<_> = sources
                .iter()
                .enumerate()
                .filter(|(_, source)| requested.as_ref().is_some_and(|path| source.path == *path))
                .collect();
            match found.as_slice() {
                [] => (None, Some("not_in_scope")),
                [(index, source)] if line <= source.text.lines().count() => {
                    (Some((*index, line)), None)
                }
                [_] => (None, Some("line_out_of_range")),
                _ => (None, Some("ambiguous_path")),
            }
        }
    };
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
    let mut pieces = Vec::new();
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
        let priority = range
            .evidence
            .iter()
            .map(|e| e.kind)
            .min()
            .unwrap_or(Kind::TextualReference);
        let evidence: Vec<_> = range.evidence.into_iter().map(|e| {
            json!({"kind": e.kind, "line": e.line, "status": e.kind.status(), "reasons": e.reasons})
        }).collect();
        pieces.push(Piece {
            priority,
            json: json!({"path": source.display, "start_line": range.start, "end_line": end, "content": content, "evidence": evidence}),
        });
    }
    let anchor = match observed {
        Some((index, line)) => {
            json!({"kind": input, "status": "observed", "path": sources[index].display, "line": line})
        }
        None if at.is_some() => {
            json!({"kind": input, "status": "unverified", "reason": anchor_reason})
        }
        None => json!({"kind": input, "status": "query"}),
    };
    let symbol_status = match (symbol, definitions, warnings.is_empty()) {
        (None, _, _) => "not_inferred",
        (Some(_), 2.., _) => "ambiguous",
        (Some(_), _, false) => "partial",
        (Some(_), 1, true) => "one_candidate",
        (Some(_), _, true) => "not_observed",
    };
    let base = json!({"schema": SCHEMA, "command": "context", "source": "local_rust_snapshot",
        "root": safe_display(&root, &cwd).unwrap_or_else(|| "restricted".into()),
        "anchor": anchor, "symbol": symbol, "symbol_status": symbol_status,
        "definition_count": definitions, "rust_files_read": sources.len(),
        "files_skipped": skipped_files});
    Ok(Shape {
        base: &base,
        warnings: &warnings,
        pieces: &pieces,
        budget: args.budget,
    }
    .fit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str, text: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ff-ctx-unit-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let dir = dir.canonicalize().unwrap();
        let file = dir.join("a.rs");
        fs::write(&file, text).unwrap();
        (dir, file)
    }

    #[test]
    fn file_changed_during_read_is_dropped() {
        let (dir, file) = temp_file("changed", "fn a() {}\n");
        assert!(load_source(&file, &dir, &dir, &|| {}).is_ok());
        let result = load_source(&file, &dir, &dir, &|| {
            fs::write(&file, "fn a() {}\nfn appended_while_reading() {}\n").unwrap();
        });
        assert_eq!(result.err(), Some("file_changed_during_read"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn replaced_file_during_read_is_dropped() {
        let (dir, file) = temp_file("swap", "fn a() {}\n");
        let result = load_source(&file, &dir, &dir, &|| {
            fs::remove_file(&file).unwrap();
            fs::write(&file, "fn b() {}\nfn c() {}\n").unwrap();
        });
        assert_eq!(result.err(), Some("file_changed_during_read"));
        fs::remove_dir_all(dir).unwrap();
    }
}

//! Experimental Rust validation suggestions; no suggestion is executed.
mod git;
mod rust;

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use clap::Args;
use serde::Serialize;

use crate::secrets::{is_secret_file, mask_secrets};
use crate::{util, CliError};
use git::{error, Blobs, Entry, Git, MAX_FILE};

const MAX_PATHS: usize = 1024;
const MAX_BYTES: usize = 16 * 1024 * 1024;

/// Identificador do contrato experimental do plano de validação.
const SCHEMA: &str = "ff/validate-plan/experimental-2";

#[derive(Args, Debug, PartialEq, Eq)]
pub struct ValidatePlanArgs {
    /// Autoriza o uso do contrato experimental.
    #[arg(long)]
    experimental: bool,
    /// Raiz do checkout Git, relativa ao diretório atual.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Limita mudanças a um caminho literal relativo à raiz (arquivo ou diretório).
    #[arg(long, default_value = ".")]
    path: String,
    /// Compara esta revisão local ao índice, em vez de HEAD.
    #[arg(long)]
    base: Option<String>,
}

pub(crate) fn run(args: &ValidatePlanArgs) -> Result<(), CliError> {
    if !args.experimental {
        return Err(CliError::InvalidArgument(
            "use --experimental para o contrato de plano de validação".into(),
        ));
    }
    println!(
        "{}",
        build(&args.root, &args.path, args.base.as_deref(), None)?
    );
    Ok(())
}

/// Constrói o plano `SCHEMA` a partir de uma raiz explícita. CLI e MCP compartilham esta
/// construção, então as duas superfícies não podem divergir no conteúdo do plano; `budget` só
/// existe no MCP e nunca pode estreitar a validação sugerida.
pub(crate) fn build(
    root: &Path,
    path: &str,
    base: Option<&str>,
    budget: Option<usize>,
) -> Result<String, CliError> {
    if !git::relative(path) || hidden(path) {
        return Err(CliError::InvalidArgument(
            "use um --path literal dentro da raiz".into(),
        ));
    }
    let root = root.canonicalize().map_err(|_| error("raiz inacessível"))?;
    let plan = fit_budget(plan(path, base, &root)?, budget);
    serde_json::to_string(&plan).map_err(|_| error("falha ao serializar plano"))
}

#[derive(Serialize)]
struct Plan {
    schema: &'static str,
    /// Nenhuma sugestão é executada pelo FastFile; quem decide é o harness.
    executed: bool,
    /// A evidência de validação só existe depois de o harness rodar os testes.
    evidence_status: &'static str,
    /// Só o MCP trunca, por `budget`; o CLI sempre entrega o plano completo.
    truncated: bool,
    root: String,
    scope: String,
    base_commit: String,
    coverage: Coverage,
    changes: Vec<Change>,
    changes_omitted: usize,
    impacted_packages: BTreeSet<String>,
    broadening_reasons: BTreeSet<&'static str>,
    first_check: Option<rust::Suggestion>,
    additional: Vec<rust::Suggestion>,
}

#[derive(Serialize)]
struct Coverage {
    base_to_index: &'static str,
    index_to_worktree: &'static str,
    untracked: &'static str,
    complete: bool,
}

#[derive(Serialize)]
struct Change {
    layer: &'static str,
    status: &'static str,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    ranges: Vec<Range>,
    symbols: BTreeSet<String>,
    /// O arquivo se declara gerado; símbolos e testes próximos não são confiáveis.
    #[serde(skip_serializing_if = "is_false")]
    generated: bool,
}

#[derive(Serialize)]
struct Range {
    old_start: usize,
    old_lines: usize,
    new_start: usize,
    new_lines: usize,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl Plan {
    fn size(&self) -> usize {
        serde_json::to_string(self).map_or(usize::MAX, |text| text.len())
    }
}

/// Truncar o plano pelo orçamento nunca pode estreitar a validação. Perdemos detalhe, marcamos a
/// cobertura como incompleta e mantemos apenas a sugestão mais ampla disponível: uma seleção
/// menor, sem a evidência que a sustentava, pareceria cobertura completa sem ser.
fn fit_budget(mut plan: Plan, budget: Option<usize>) -> Plan {
    let Some(tokens) = budget else {
        return plan;
    };
    let max_bytes = tokens.saturating_mul(4);
    if plan.size() <= max_bytes {
        return plan;
    }
    let broadest = match plan.first_check.as_ref() {
        Some(check) if check.scope == "workspace" => plan.first_check.clone(),
        _ => plan
            .additional
            .iter()
            .rev()
            .find(|check| check.scope == "workspace")
            .cloned(),
    };
    plan.changes_omitted = plan.changes.len();
    plan.changes.clear();
    plan.additional.clear();
    plan.first_check = broadest;
    plan.coverage.complete = false;
    plan.broadening_reasons.insert("budget_truncated");
    plan.truncated = true;
    if plan.size() > max_bytes {
        // Sem espaço nem para a suíte ampla: não recomendar nada é melhor que recomendar de menos.
        plan.first_check = None;
    }
    plan
}

fn hidden(path: &str) -> bool {
    path.split('/').any(|p| {
        is_secret_file(p)
            || matches!(
                p.to_ascii_lowercase().as_str(),
                ".git" | "target" | "node_modules"
            )
    }) || mask_secrets(path).1 != 0
}

fn display(root: &Path, path: &str) -> String {
    mask_secrets(&util::display_path(&root.join(path).to_string_lossy())).0
}

fn within(path: &str, scope: &str) -> bool {
    scope == "." || path == scope || path.strip_prefix(scope).is_some_and(|s| s.starts_with('/'))
}

// None is ambiguous (permission, reparse point, special file, or byte budget);
// Some(None) is an observed missing file. Never follow symlinks/junctions.
fn worktree(root: &Path, relative: &str, remaining: &mut usize) -> Option<Option<Vec<u8>>> {
    let mut path = root.to_owned();
    let parts = Path::new(relative).components();
    let count = parts.clone().count();
    for (i, part) in parts.enumerate() {
        path.push(part);
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Some(None),
            Err(_) => return None,
        };
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if meta.file_attributes() & 0x400 != 0 {
                return None;
            }
        }
        if (i + 1 != count && !meta.is_dir()) || (i + 1 == count && !meta.is_file()) {
            return None;
        }
    }
    if !path.canonicalize().ok()?.starts_with(root) {
        return None;
    }
    let cap = MAX_FILE.min(*remaining);
    let mut bytes = Vec::new();
    File::open(path)
        .ok()?
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > cap {
        return None;
    }
    *remaining -= bytes.len();
    Some(Some(bytes))
}

fn working_text(bytes: &[u8]) -> Option<String> {
    if ff_diff::is_binary(bytes) {
        return None;
    }
    Some(std::str::from_utf8(bytes).ok()?.replace("\r\n", "\n"))
}

/// Marca de arquivo gerado nas primeiras linhas: o código veio de uma ferramenta, então a
/// referência de símbolo e o teste adjacente não provam nada sobre a mudança.
fn generated(text: &str) -> bool {
    text.lines().take(5).any(|line| {
        let line = line.to_ascii_lowercase();
        line.contains("@generated")
            || line.contains("code generated by")
            || line.contains("automatically generated")
            || line.contains("do not edit")
    })
}

fn emit(
    layer: &'static str,
    status: &'static str,
    path: &str,
    old: Option<&[u8]>,
    new: Option<&[u8]>,
) -> Change {
    let mut result = Change {
        layer,
        status,
        path: path.into(),
        old_path: None,
        ranges: vec![],
        symbols: BTreeSet::new(),
        generated: false,
    };
    let (Some(old), Some(new)) = (
        working_text(old.unwrap_or_default()),
        working_text(new.unwrap_or_default()),
    ) else {
        return result;
    };
    result.generated = generated(&new);
    if !path.ends_with(".rs") {
        return result;
    }
    let diff = ff_diff::diff_texts("", "", &old, &new, 0);
    let symbols = [
        ff_sym::extract_symbols(Path::new(path), &old),
        ff_sym::extract_symbols(Path::new(path), &new),
    ];
    for hunk in diff.hunks {
        for (from, len, symbols) in [
            (hunk.old_start, hunk.old_lines, &symbols[0]),
            (hunk.new_start, hunk.new_lines, &symbols[1]),
        ] {
            if len == 0 {
                continue;
            }
            // ff-sym exposes declaration positions. Use declarations in the hunk plus the closest preceding one (heuristic).
            for s in symbols
                .iter()
                .filter(|s| s.line >= from && s.line < from.saturating_add(len))
                .chain(
                    symbols
                        .iter()
                        .filter(|s| s.line < from)
                        .max_by_key(|s| s.line),
                )
            {
                if mask_secrets(&s.name).1 == 0 {
                    result.symbols.insert(s.name.clone());
                }
            }
        }
        result.ranges.push(Range {
            old_start: hunk.old_start,
            old_lines: hunk.old_lines,
            new_start: hunk.new_start,
            new_lines: hunk.new_lines,
        });
    }
    result
}

fn read_entry(
    blobs: &mut Blobs,
    e: Option<&Entry>,
    remaining: &mut usize,
) -> Result<Option<Option<Vec<u8>>>, CliError> {
    match e {
        None => Ok(Some(None)),
        Some(entry) => Ok(blobs.read(entry, remaining)?.map(Some)),
    }
}

fn staged(
    base: &BTreeMap<String, Entry>,
    index: &BTreeMap<String, Entry>,
) -> BTreeMap<String, String> {
    let mut deleted = BTreeMap::<&str, Vec<&str>>::new();
    let mut added = BTreeMap::<&str, Vec<&str>>::new();
    for (path, entry) in base.iter().filter(|(p, _)| !index.contains_key(*p)) {
        deleted.entry(&entry.oid).or_default().push(path);
    }
    for (path, entry) in index.iter().filter(|(p, _)| !base.contains_key(*p)) {
        added.entry(&entry.oid).or_default().push(path);
    }
    added
        .iter()
        .filter_map(
            |(id, new)| match (deleted.get(id)?.as_slice(), new.as_slice()) {
                ([old], [new])
                    if base[*old].mode == index[*new].mode && !hidden(old) && !hidden(new) =>
                {
                    Some(((*new).to_owned(), (*old).to_owned()))
                }
                _ => None,
            },
        )
        .collect()
}

fn plan(scope: &str, base_ref: Option<&str>, root: &Path) -> Result<Plan, CliError> {
    let git = Git::open(root.to_owned())?;
    let base_commit = git.commit(base_ref.unwrap_or("HEAD"))?;
    let base = git.tree(&base_commit)?;
    let index = git.index()?;
    let (others, others_incomplete) = git.others()?;
    let mut remaining = MAX_BYTES;
    let mut reasons = BTreeSet::new();
    let mut complete = !base.incomplete && !index.incomplete && !others_incomplete;
    let mut paths: BTreeSet<_> = base
        .entries
        .keys()
        .chain(index.entries.keys())
        .chain(&index.unknown)
        .chain(&others)
        .collect();
    let renames = staged(&base.entries, &index.entries);
    let rename_sources: BTreeSet<_> = renames.values().collect();
    paths.retain(|p| within(p, scope) || renames.get(*p).is_some_and(|old| within(old, scope)));
    if paths.len() > MAX_PATHS {
        complete = false;
    }
    let mut blobs = git.blobs()?;
    let mut changes = vec![];
    let first_layer = if base_ref.is_some() {
        "base_to_index"
    } else {
        "staged"
    };
    for path in paths.into_iter().take(MAX_PATHS) {
        if hidden(path) {
            // A change we refuse to inspect must not look like a clean, complete plan.
            if base.entries.get(path) != index.entries.get(path) || index.unknown.contains(path) {
                complete = false;
                reasons.insert("excluded_path_change");
            }
            continue;
        }
        if index.unknown.contains(path) {
            complete = false;
            continue;
        }
        let before = base.entries.get(path);
        let entry = index.entries.get(path);
        let untracked = others.contains(path);
        let mut indexed = None;
        // A scope-crossing rename is emitted once at its destination; no unrelated working bytes read.
        let in_scope = within(path, scope);
        if entry.is_some() && (in_scope || !renames.contains_key(path)) {
            indexed = read_entry(&mut blobs, entry, &mut remaining)?.flatten();
        }
        if let Some(old) = renames.get(path) {
            let mut change = emit(first_layer, "renamed", path, None, None);
            change.old_path = Some(old.clone());
            changes.push(change);
        } else if before != entry && !rename_sources.contains(path) {
            let old = read_entry(&mut blobs, before, &mut remaining)?;
            let new = if entry.is_none() {
                Some(None)
            } else {
                indexed.as_deref().map(Some)
            };
            let status = match (before, entry) {
                (None, _) => "added",
                (_, None) => "deleted",
                _ => "modified",
            };
            let change = match (old, new) {
                (Some(old), Some(new)) => emit(first_layer, status, path, old.as_deref(), new),
                _ => {
                    complete = false;
                    emit(first_layer, status, path, None, None)
                }
            };
            changes.push(change);
        }
        if !in_scope || (entry.is_none() && !untracked) {
            continue;
        }
        let Some(current) = worktree(root, path, &mut remaining) else {
            complete = false;
            continue;
        };
        if entry.is_some() && indexed.is_none() {
            complete = false;
            continue;
        }
        match (indexed.as_deref(), current.as_deref()) {
            (Some(old), Some(new))
                if old != new
                    && (working_text(old).is_none() || working_text(old) != working_text(new)) =>
            {
                changes.push(emit("unstaged", "modified", path, Some(old), Some(new)));
            }
            (Some(old), None) => changes.push(emit("unstaged", "deleted", path, Some(old), None)),
            (None, Some(new)) => changes.push(emit("untracked", "added", path, None, Some(new))),
            (None, None) if untracked => complete = false,
            _ => {}
        }
    }
    if !complete {
        reasons.insert("partial_diff");
    }
    if changes
        .iter()
        .any(|c| c.status == "deleted" || c.status == "renamed")
    {
        reasons.insert("removed_or_renamed_path");
    }
    if changes.iter().any(|c| c.generated) {
        reasons.insert("generated_code_change");
    }
    if changes.iter().any(|c| {
        !c.path.ends_with(".rs")
            || c.path.rsplit('/').next() == Some("build.rs")
            || c.old_path.as_ref().is_some_and(|p| !p.ends_with(".rs"))
    }) {
        reasons.insert("configuration_or_non_rust_change");
    }
    let paths: BTreeSet<_> = index
        .entries
        .keys()
        .chain(&others)
        .filter(|p| !hidden(p))
        .map(String::as_str)
        .collect();
    let (impacted_packages, suggestions) =
        rust::suggest(root, &paths, &changes, &mut reasons, &mut remaining);
    changes.sort_by_key(|change| match change.layer {
        "unstaged" => 1,
        "untracked" => 2,
        _ => 0,
    });
    for change in &mut changes {
        change.path = display(root, &change.path);
        change.old_path = change.old_path.as_ref().map(|p| display(root, p));
    }
    // The suggestion list is ordered narrowest first, so the first is the check to run before
    // anything else and the rest are the broader suites that back it up.
    let mut suggestions = suggestions.into_iter();
    Ok(Plan {
        schema: SCHEMA,
        executed: false,
        evidence_status: "not_executed",
        truncated: false,
        root: display(root, "."),
        scope: display(root, scope),
        base_commit,
        coverage: Coverage {
            base_to_index: first_layer,
            index_to_worktree: "unstaged",
            untracked: "non_ignored",
            complete,
        },
        changes,
        changes_omitted: 0,
        impacted_packages,
        broadening_reasons: reasons,
        first_check: suggestions.next(),
        additional: suggestions.collect(),
    })
}

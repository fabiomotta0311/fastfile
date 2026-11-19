use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use ff_sym::SymbolKind;
use serde::Serialize;
use toml::Value;

use super::{display, worktree, Change, MAX_PATHS};
use crate::secrets::mask_secrets;

#[derive(Clone, Serialize)]
pub(super) struct Suggestion {
    pub(super) scope: &'static str,
    argv: Vec<String>,
    evidence: Vec<Evidence>,
}

#[derive(Clone, Serialize)]
struct Evidence {
    source: &'static str,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    line: Option<usize>,
}

struct Package<'a> {
    manifest: &'a str,
    dir: &'a str,
    name: String,
    value: Value,
}

fn ident(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        && mask_secrets(value).1 == 0
}

fn text(root: &Path, path: &str, remaining: &mut usize) -> Option<String> {
    String::from_utf8(worktree(root, path, remaining)??).ok()
}

fn dependencies(value: &Value) -> impl Iterator<Item = (&str, &Value)> {
    ["dependencies", "dev-dependencies", "build-dependencies"]
        .into_iter()
        .filter_map(|k| value.get(k)?.as_table())
        .flatten()
        .map(|(name, spec)| (name.as_str(), spec))
}

fn locate_path(dir: &str, path: &str) -> Option<PathBuf> {
    let mut local = PathBuf::from(dir);
    for part in Path::new(path).components() {
        match part {
            Component::ParentDir if local.pop() => {}
            Component::CurDir => {}
            Component::Normal(p) => local.push(p),
            _ => return None,
        }
    }
    Some(local)
}

fn owner<'a, 'p>(packages: &'p [Package<'a>], path: &str) -> Option<&'p Package<'a>> {
    packages
        .iter()
        .filter(|p| p.dir.is_empty() || super::within(path, p.dir))
        .max_by_key(|p| p.dir.len())
}

impl Suggestion {
    fn new(scope: &'static str, args: &[&str], evidence: Vec<Evidence>) -> Self {
        let argv = ["cargo", "test"]
            .into_iter()
            .chain(args.iter().copied())
            .chain(["--locked"])
            .map(str::to_owned)
            .collect();
        Self {
            scope,
            argv,
            evidence,
        }
    }
}

fn evidence(root: &Path, path: &str, source: &'static str, line: Option<usize>) -> Evidence {
    Evidence {
        source,
        path: display(root, path),
        line,
    }
}

pub(super) fn suggest(
    root: &Path,
    paths: &BTreeSet<&str>,
    changes: &[Change],
    reasons: &mut BTreeSet<&'static str>,
    remaining: &mut usize,
) -> (BTreeSet<String>, Vec<Suggestion>) {
    if changes.is_empty() && reasons.is_empty() {
        return (BTreeSet::new(), vec![]);
    }
    let mut packages = vec![];
    let mut workspace = None;
    let mut has_root_manifest = false;
    if paths.len() > MAX_PATHS {
        reasons.insert("evidence_limit");
    }
    for &path in paths
        .iter()
        .filter(|p| p.rsplit('/').next() == Some("Cargo.toml"))
        .take(MAX_PATHS)
    {
        let Some(value) = text(root, path, remaining).and_then(|t| t.parse::<Value>().ok()) else {
            reasons.insert("manifest_unavailable");
            continue;
        };
        if path == "Cargo.toml" {
            has_root_manifest = true;
            workspace = value.get("workspace").cloned();
        }
        let Some(name) = value
            .get("package")
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str)
        else {
            continue;
        };
        if !ident(name) || packages.iter().any(|p: &Package<'_>| p.name == name) {
            reasons.insert("ambiguous_package");
            continue;
        }
        let dir = path.rsplit_once('/').map_or("", |(dir, _)| dir);
        packages.push(Package {
            manifest: path,
            dir,
            name: name.to_owned(),
            value,
        });
    }
    let mut impacted = BTreeSet::new();
    for change in changes {
        for path in std::iter::once(change.path.as_str()).chain(change.old_path.as_deref()) {
            if let Some(package) = owner(&packages, path) {
                impacted.insert(package.name.clone());
            } else if path.ends_with(".rs") {
                reasons.insert("unmapped_rust_path");
            }
        }
        if change.path.ends_with(".rs") && change.symbols.is_empty() {
            reasons.insert("unmapped_symbols");
        }
    }
    let mut edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for p in &packages {
        let target_tables = p
            .value
            .get("target")
            .and_then(Value::as_table)
            .into_iter()
            .flat_map(|t| t.values());
        for (key, spec) in dependencies(&p.value).chain(target_tables.flat_map(dependencies)) {
            let inherited = spec.get("workspace").and_then(Value::as_bool) == Some(true);
            let spec = if inherited {
                let Some(spec) = workspace
                    .as_ref()
                    .and_then(|w| w.get("dependencies"))
                    .and_then(|d| d.get(key))
                else {
                    reasons.insert("unresolved_path_dependency");
                    continue;
                };
                spec
            } else {
                spec
            };
            let Some(path) = spec.get("path").and_then(Value::as_str) else {
                continue;
            };
            let Some(dep) = locate_path(if inherited { "" } else { p.dir }, path).and_then(|dir| {
                packages
                    .iter()
                    .find(|candidate| Path::new(candidate.dir) == dir)
            }) else {
                reasons.insert("unresolved_path_dependency");
                continue;
            };
            edges.entry(&dep.name).or_default().insert(&p.name);
        }
    }
    let mut pending: Vec<_> = impacted.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        if let Some(dependents) = edges.get(name.as_str()) {
            for &dep in dependents {
                if impacted.insert(dep.to_owned()) {
                    pending.push(dep.to_owned());
                }
            }
        }
    }
    if !has_root_manifest {
        reasons.insert("no_root_manifest");
    }
    let ci = ".github/workflows/ci.yml";
    let ci_line = paths
        .contains(ci)
        .then(|| text(root, ci, remaining))
        .flatten()
        .and_then(|t| {
            t.lines()
                .position(|line| {
                    matches!(
                        line.trim(),
                        "run: cargo test --workspace --locked"
                            | "- run: cargo test --workspace --locked"
                    )
                })
                .map(|n| n + 1)
        });
    let workspace_evidence = if let Some(line) = ci_line {
        vec![evidence(root, ci, "known_ci_test_suite", Some(line))]
    } else if has_root_manifest {
        vec![evidence(root, "Cargo.toml", "manifest", None)]
    } else {
        vec![]
    };
    let broader = Suggestion::new("workspace", &["--workspace"], workspace_evidence);
    if !reasons.is_empty() {
        return (
            impacted,
            if has_root_manifest {
                vec![broader]
            } else {
                vec![]
            },
        );
    }
    let names: BTreeSet<_> = changes
        .iter()
        .flat_map(|c| &c.symbols)
        .filter(|n| ident(n))
        .map(String::as_str)
        .collect();
    let mut suggestions = Vec::new();
    // Default integration-test targets only. Don't guess custom target names or feature flags.
    for &path in paths.iter().filter(|p| p.ends_with(".rs")).take(MAX_PATHS) {
        let Some(pkg) = owner(&packages, path) else {
            continue;
        };
        if !impacted.contains(&pkg.name)
            || pkg.value["package"]
                .get("autotests")
                .and_then(Value::as_bool)
                == Some(false)
            || pkg.value.get("test").is_some()
        {
            continue;
        }
        let local = if pkg.dir.is_empty() {
            path
        } else {
            &path[pkg.dir.len() + 1..]
        };
        let Some(target) = local
            .strip_prefix("tests/")
            .and_then(|p| p.strip_suffix(".rs"))
            .filter(|p| ident(p))
        else {
            continue;
        };
        let Some(source) = text(root, path, remaining) else {
            reasons.insert("test_evidence_unavailable");
            continue;
        };
        let symbols = ff_sym::extract_symbols(Path::new(path), &source);
        let lines: Vec<_> = source.lines().collect();
        for symbol in symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function && ident(&s.name))
        {
            if symbol.line < 2 || lines[symbol.line - 2].trim() != "#[test]" {
                continue;
            }
            let end = symbols
                .iter()
                .filter(|s| s.line > symbol.line)
                .map(|s| s.line - 1)
                .min()
                .unwrap_or(lines.len());
            let matches = lines[symbol.line - 1..end]
                .iter()
                .flat_map(|l| l.split(|c: char| !c.is_ascii_alphanumeric() && c != '_'))
                .any(|word| names.contains(word));
            if matches {
                suggestions.push(Suggestion::new(
                    "first_test",
                    &["-p", &pkg.name, "--test", target, &symbol.name],
                    vec![
                        evidence(
                            root,
                            path,
                            "test_symbol_reference_heuristic",
                            Some(symbol.line),
                        ),
                        evidence(root, pkg.manifest, "manifest", None),
                    ],
                ));
                break;
            }
        }
        if !suggestions.is_empty() {
            break;
        }
    }
    if !reasons.is_empty() {
        return (impacted, vec![broader]);
    }
    for p in &packages {
        if impacted.contains(&p.name) {
            suggestions.push(Suggestion::new(
                "package",
                &["-p", &p.name],
                vec![evidence(root, p.manifest, "manifest", None)],
            ));
        }
    }
    suggestions.push(broader);
    (impacted, suggestions)
}

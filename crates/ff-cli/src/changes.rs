//! Experimental, caller-owned endpoint snapshots. Nothing from the state is used to open files.
use crate::{map::is_build_junk_dir, schema, secrets, util::display_path_from, CliError};
use cap_fs_ext::{DirExt, FollowSymlinks, MetadataExt, OpenOptionsFollowExt};
use cap_std::{
    ambient_authority,
    fs::{Dir, File, Metadata, OpenOptions},
};
use clap::Args;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const VERSION: u32 = 1;
const TTL: u64 = 3600;
const MAX_STATE: usize = 2 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_DEPTH: usize = 32;
const MAX_PATH: usize = 1024;
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_CONTROL: u64 = 64 * 1024;
const LIMITATIONS: &[&str] = &[
    "endpoint_comparison_only",
    "transient_changes_not_observable",
    "scan_not_atomic",
    "metadata_only_changes_not_observable",
    "renames_not_inferred",
    "filtered_paths_not_observed",
];

#[derive(Args, Debug, PartialEq, Eq)]
pub struct ScopeArgs {
    /// Habilita o contrato experimental (somente ff/2).
    #[arg(long, required = true)]
    experimental: bool,
    /// Diretório local autorizado, relativo à pasta atual ou absoluto.
    #[arg(long, default_value = ".")]
    root: PathBuf,
    /// Inclui os subdiretórios target/, build/ e dist/ diretamente sob a raiz.
    #[arg(long)]
    include_artifacts: bool,
}

#[derive(Args, Debug, PartialEq, Eq)]
pub struct ChangesArgs {
    #[command(flatten)]
    pub scope: ScopeArgs,
    /// Leia do stdin o objeto checkpoint emitido por fastfile checkpoint.
    #[arg(long, value_parser = ["-"])]
    since: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    issued_at: u64,
    scope: String,
    artifacts: bool,
    policy: String,
    files: BTreeMap<String, String>,
}

#[derive(Clone, Copy)]
struct Unobserved(&'static str);
impl From<io::Error> for Unobserved {
    fn from(error: io::Error) -> Self {
        // ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION: another process denied access.
        #[cfg(windows)]
        if matches!(error.raw_os_error(), Some(32 | 33)) {
            return Self("access_denied");
        }
        Self(match error.kind() {
            io::ErrorKind::PermissionDenied => "access_denied",
            io::ErrorKind::NotFound => "tree_changed_during_scan",
            _ => "io_error",
        })
    }
}
type ScanResult<T> = Result<T, Unobserved>;

pub fn run(args: &ScopeArgs, compare: bool) -> Result<(), CliError> {
    if !schema::is_v2() {
        return Err(CliError::InvalidArgument(
            "checkpoint e changes exigem ff/2".into(),
        ));
    }
    let command = if compare { "changes" } else { "checkpoint" };
    let result = execute(args, compare).unwrap_or_else(|Unobserved(reason)| {
        json!({"status": "indeterminate", "coverage": "incomplete", "reason": reason})
    });
    let mut result = result;
    result["command"] = json!(command);
    result["schema"] = json!(schema::SCHEMA_VERSION_V2);
    result["source"] = json!("portable_snapshot");
    result["limitations"] = json!(LIMITATIONS);
    println!("{result}");
    Ok(())
}

fn execute(args: &ScopeArgs, compare: bool) -> ScanResult<Value> {
    let previous = if compare {
        let mut input = Vec::new();
        io::stdin()
            .lock()
            .take(MAX_STATE as u64 + 1)
            .read_to_end(&mut input)?;
        let state = (input.len() <= MAX_STATE)
            .then(|| serde_json::from_slice::<State>(&input).ok())
            .flatten();
        let Some(state) = state else {
            return Ok(recreate("invalid_checkpoint"));
        };
        if state.version != VERSION {
            return Ok(recreate("checkpoint_version"));
        }
        if !valid_state(&state) {
            return Ok(recreate("invalid_checkpoint"));
        }
        let expiry = state.issued_at.checked_add(TTL);
        let now = unix_time()?;
        if expiry.is_none_or(|e| now >= e) || now < state.issued_at {
            return Ok(recreate("expired_checkpoint"));
        }
        if state.artifacts != args.include_artifacts {
            return Ok(recreate("filter_policy_changed"));
        }
        Some(state)
    } else {
        None
    };

    let issued_at = unix_time()?;
    let root = local_root(&args.root)?;
    let dir = open_root(&root)?;
    let mut scan = Scan::default();
    let scope = scope_id(&root, &dir, &mut scan)?;
    if previous.as_ref().is_some_and(|p| p.scope != scope) {
        return Ok(recreate("root_or_checkout_changed"));
    }
    scan.walk(
        &dir,
        "",
        &mut Vec::new(),
        args.include_artifacts,
        false,
        dir.dir_metadata()?.dev(),
    )?;
    // Catch a root/worktree replacement during collection, not just between commands.
    let current = open_root(&root)?;
    if scope_id(&root, &current, &mut scan)? != scope {
        return Err(Unobserved("tree_changed_during_scan"));
    }
    let policy =
        digest(&serde_json::to_vec(&scan.rules).map_err(|_| Unobserved("serialization_error"))?);
    let state = State {
        version: VERSION,
        issued_at,
        scope,
        artifacts: args.include_artifacts,
        policy,
        files: scan.files,
    };
    if serde_json::to_vec(&state)
        .map_err(|_| Unobserved("serialization_error"))?
        .len()
        > MAX_STATE
    {
        return Err(Unobserved("state_budget"));
    }
    if let Some(previous) = previous {
        if unix_time()? >= previous.issued_at + TTL {
            return Ok(recreate("expired_checkpoint"));
        }
        if previous.policy != state.policy {
            return Ok(recreate("filter_policy_changed"));
        }
        return compare_states(&root, &previous.files, &state.files);
    }
    Ok(
        json!({"status": "checkpoint_created", "coverage": "observed_scope", "expires_at": issued_at + TTL, "checkpoint": state}),
    )
}

fn recreate(reason: &str) -> Value {
    json!({"status": "recreate_checkpoint", "coverage": "unavailable", "reason": reason,
        "action": "run_checkpoint_again"})
}

fn unix_time() -> ScanResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .map_err(|_| Unobserved("clock_error"))
}

fn local_root(path: &Path) -> ScanResult<PathBuf> {
    let path = std::env::current_dir()?.join(path);
    #[cfg(windows)]
    fn local(path: &Path) -> bool {
        use std::path::{Component, Prefix};
        matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_)))
            && path.is_absolute()
    }
    #[cfg(windows)]
    if !local(&path) {
        return Err(Unobserved("unsupported_root"));
    }
    let root = path.canonicalize()?;
    #[cfg(windows)]
    if !local(&root) {
        return Err(Unobserved("unsupported_root"));
    }
    if root.components().any(|c| matches!(c, std::path::Component::Normal(n) if n.to_str().is_some_and(|n| secret_name(n) || n.eq_ignore_ascii_case(".git")))) {
        return Err(Unobserved("sensitive_root"));
    }
    Ok(root)
}

fn open_root(path: &Path) -> ScanResult<Dir> {
    use std::path::Component;
    let mut anchor = PathBuf::new();
    let mut parts = path.components().peekable();
    while matches!(
        parts.peek(),
        Some(Component::Prefix(_) | Component::RootDir)
    ) {
        anchor.push(parts.next().expect("checked component").as_os_str());
    }
    if parts.peek().is_none() {
        return Err(Unobserved("unsupported_root"));
    }
    let mut dir = Dir::open_ambient_dir(anchor, ambient_authority())?;
    for part in parts {
        dir = dir.open_dir_nofollow(part.as_os_str())?;
        if is_link(&dir.dir_metadata()?) {
            return Err(Unobserved("link_or_reparse_point"));
        }
    }
    Ok(dir)
}

fn identity(m: &Metadata) -> (u64, u64) {
    (m.dev(), m.ino())
}

fn scope_id(path: &Path, dir: &Dir, scan: &mut Scan) -> ScanResult<String> {
    let path = path.to_str().ok_or(Unobserved("unsupported_root"))?;
    let root = dir.dir_metadata()?;
    if is_link(&root) {
        return Err(Unobserved("unsupported_root"));
    }
    let marker = match dir.symlink_metadata(".git") {
        Ok(m) if is_link(&m) => return Err(Unobserved("unsupported_checkout_marker")),
        Ok(m) if m.is_dir() => {
            let git = dir.open_dir_nofollow(".git")?;
            let meta = git.dir_metadata()?;
            if is_link(&meta) || meta.dev() != root.dev() {
                return Err(Unobserved("unsupported_checkout_marker"));
            }
            json!(["git_directory", identity(&meta)])
        }
        Ok(_) => json!([
            "worktree_marker",
            digest(
                &scan
                    .read_control(dir, ".git")?
                    .ok_or(Unobserved("tree_changed_during_scan"))?
            )
        ]),
        Err(e) if e.kind() == io::ErrorKind::NotFound => json!("directory"),
        Err(e) => return Err(e.into()),
    };
    Ok(digest(
        json!([std::env::consts::OS, path, identity(&root), marker])
            .to_string()
            .as_bytes(),
    ))
}

fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// SHA-256 em hexadecimal minúsculo (o sha2 0.11 não implementa `LowerHex` no resultado).
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes
        .iter()
        .fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}
fn valid_hash(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}
fn valid_state(s: &State) -> bool {
    valid_hash(&s.scope)
        && valid_hash(&s.policy)
        && s.files.len() <= MAX_ENTRIES
        && s.files.iter().all(|(p, h)| {
            p.len() <= MAX_PATH
                && p.split('/').count() <= MAX_DEPTH
                && p.split('/')
                    .all(|n| portable_name(n) && !secret_name(n) && !n.eq_ignore_ascii_case(".git"))
                && valid_hash(h)
        })
}
fn portable_name(name: &str) -> bool {
    !name.is_empty()
        && !name.ends_with(['.', ' '])
        && !name.contains(['\\', ':', '/', '<', '>', '"', '|', '?', '*'])
        && !name.chars().any(char::is_control)
}
fn secret_name(name: &str) -> bool {
    secrets::is_secret_file(name) || secrets::mask_secrets(name).1 != 0
}

fn compare_states(
    root: &Path,
    old: &BTreeMap<String, String>,
    new: &BTreeMap<String, String>,
) -> ScanResult<Value> {
    let cwd = std::env::current_dir()?.canonicalize()?;
    let mut events = Vec::new();
    let paths: std::collections::BTreeSet<_> = old.keys().chain(new.keys()).collect();
    for path in paths {
        let kind = match (old.get(path), new.get(path)) {
            (None, Some(_)) => "added",
            (Some(_), None) => "deleted",
            (Some(a), Some(b)) if a != b => "modified",
            _ => continue,
        };
        let absolute = root.join(path);
        let displayed = display_path_from(
            absolute.to_str().ok_or(Unobserved("unsupported_path"))?,
            Some(&cwd),
        );
        if displayed.split('/').any(secret_name) || secrets::mask_secrets(&displayed).1 > 0 {
            return Err(Unobserved("sensitive_display_path"));
        }
        events.push(json!({"kind": kind, "path": displayed}));
    }
    let status = if events.is_empty() {
        "no_observed_changes"
    } else {
        "observed_changes"
    };
    let result = json!({"status": status, "coverage": "observed_scope", "events": events});
    if result.to_string().len() > MAX_STATE {
        return Err(Unobserved("output_budget"));
    }
    Ok(result)
}

#[derive(Default)]
struct Scan {
    files: BTreeMap<String, String>,
    rules: BTreeMap<String, String>,
    entries: usize,
    bytes: u64,
}

impl Scan {
    fn open(&self, dir: &Dir, name: &str, meta: &Metadata) -> ScanResult<File> {
        if is_link(meta) || !meta.is_file() {
            return Err(Unobserved("unsupported_file_type"));
        }
        let mut options = OpenOptions::new();
        options.read(true).follow(FollowSymlinks::No);
        #[cfg(unix)]
        cap_std::fs::OpenOptionsExt::custom_flags(&mut options, libc::O_NONBLOCK);
        let file = dir.open_with(name, &options)?;
        let actual = file.metadata()?;
        if is_link(&actual) || !actual.is_file() || identity(meta) != identity(&actual) {
            return Err(Unobserved("tree_changed_during_scan"));
        }
        Ok(file)
    }

    fn charge(&mut self, bytes: u64) -> ScanResult<()> {
        self.bytes += bytes;
        if self.bytes > MAX_BYTES {
            return Err(Unobserved("byte_budget"));
        }
        Ok(())
    }

    fn read_control(&mut self, dir: &Dir, name: &str) -> ScanResult<Option<Vec<u8>>> {
        let meta = match dir.symlink_metadata(name) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if meta.len() > MAX_CONTROL {
            return Err(Unobserved("control_budget"));
        }
        let mut file = self.open(dir, name, &meta)?;
        let mut bytes = Vec::new();
        (&mut file).take(MAX_CONTROL + 1).read_to_end(&mut bytes)?;
        self.charge(bytes.len() as u64)?;
        if bytes.len() as u64 > MAX_CONTROL {
            return Err(Unobserved("control_budget"));
        }
        verify_file(dir, name, &file, &meta, bytes.len() as u64)?;
        Ok(Some(bytes))
    }

    fn add_rules(
        &mut self,
        dir: &Dir,
        prefix: &str,
        name: &str,
        rules: &mut Vec<(String, Gitignore)>,
    ) -> ScanResult<()> {
        let Some(bytes) = self.read_control(dir, name)? else {
            return Ok(());
        };
        let text = std::str::from_utf8(&bytes).map_err(|_| Unobserved("invalid_ignore_rules"))?;
        let mut builder = GitignoreBuilder::new("");
        for line in text.trim_start_matches('\u{feff}').lines() {
            builder
                .add_line(None, line)
                .map_err(|_| Unobserved("invalid_ignore_rules"))?;
        }
        let matcher = builder
            .build()
            .map_err(|_| Unobserved("invalid_ignore_rules"))?;
        self.rules.insert(format!("{prefix}{name}"), digest(&bytes));
        rules.push((prefix.into(), matcher));
        Ok(())
    }

    fn walk(
        &mut self,
        dir: &Dir,
        prefix: &str,
        rules: &mut Vec<(String, Gitignore)>,
        opt_in: bool,
        artifact: bool,
        device: u64,
    ) -> ScanResult<()> {
        let before = dir.dir_metadata()?;
        let rule_count = rules.len();
        if !artifact {
            self.add_rules(dir, prefix, ".gitignore", rules)?;
            self.add_rules(dir, prefix, ".ignore", rules)?;
        }
        for entry in dir.entries()? {
            self.entries += 1;
            if self.entries > MAX_ENTRIES {
                return Err(Unobserved("entry_budget"));
            }
            let entry = entry?;
            let raw = entry.file_name();
            let name = raw
                .to_str()
                .filter(|n| portable_name(n))
                .ok_or(Unobserved("unsupported_path"))?;
            if secret_name(name) || name.eq_ignore_ascii_case(".git") {
                continue;
            }
            let path = format!("{prefix}{name}");
            if path.len() > MAX_PATH || path.split('/').count() > MAX_DEPTH {
                return Err(Unobserved("path_budget"));
            }
            let meta = dir.symlink_metadata(name)?;
            let artifact_root = opt_in
                && prefix.is_empty()
                && meta.is_dir()
                && matches!(name, "target" | "build" | "dist");
            if !artifact_root && meta.is_dir() && is_build_junk_dir(name) {
                continue;
            }
            if !(artifact || artifact_root)
                && rules.iter().rev().find_map(|(base, rule)| {
                    let matched = rule.matched(
                        path.strip_prefix(base).expect("ancestor prefix"),
                        meta.is_dir(),
                    );
                    (!matched.is_none()).then_some(matched.is_ignore())
                }) == Some(true)
            {
                continue;
            }
            if is_link(&meta) {
                return Err(Unobserved("link_or_reparse_point"));
            }
            // On Windows use metadata from the opened handle for file IDs.
            if meta.is_dir() {
                let child = dir.open_dir_nofollow(name)?;
                let actual = child.dir_metadata()?;
                if is_link(&actual) || actual.dev() != device {
                    return Err(Unobserved("link_or_mount"));
                }
                self.walk(
                    &child,
                    &format!("{path}/"),
                    rules,
                    opt_in,
                    artifact || artifact_root,
                    device,
                )?;
                let after = dir.open_dir_nofollow(name)?.dir_metadata()?;
                if identity(&after) != identity(&actual) {
                    return Err(Unobserved("tree_changed_during_scan"));
                }
            } else {
                let mut file = self.open(dir, name, &meta)?;
                let actual = file.metadata()?;
                if actual.dev() != device {
                    return Err(Unobserved("link_or_mount"));
                }
                if actual.len() > MAX_BYTES.saturating_sub(self.bytes) {
                    return Err(Unobserved("byte_budget"));
                }
                let mut hash = Sha256::new();
                let mut buffer = [0u8; 16 * 1024];
                let mut seen = 0;
                loop {
                    let n = file.read(&mut buffer)?;
                    if n == 0 {
                        break;
                    }
                    self.charge(n as u64)?;
                    seen += n as u64;
                    hash.update(&buffer[..n]);
                }
                verify_file(dir, name, &file, &actual, seen)?;
                self.files.insert(path, hex(&hash.finalize()));
            }
        }
        rules.truncate(rule_count);
        if before.modified()? != dir.dir_metadata()?.modified()? {
            return Err(Unobserved("tree_changed_during_scan"));
        }
        Ok(())
    }
}

fn verify_file(dir: &Dir, name: &str, file: &File, before: &Metadata, seen: u64) -> ScanResult<()> {
    let handle = file.metadata()?;
    let path = dir.symlink_metadata(name)?;
    if is_link(&path)
        || seen != before.len()
        || handle.len() != seen
        || path.len() != seen
        || before.modified()? != handle.modified()?
        || handle.modified()? != path.modified()?
        || identity(&path) != identity(&handle)
    {
        return Err(Unobserved("tree_changed_during_scan"));
    }
    Ok(())
}

fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        cap_fs_ext::OsMetadataExt::file_attributes(meta) & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.is_symlink()
    }
}

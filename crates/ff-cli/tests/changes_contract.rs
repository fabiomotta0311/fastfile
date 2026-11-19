use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Tree(PathBuf);

impl Tree {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let base = std::env::temp_dir();
        loop {
            let dir = base.join(format!(
                "ff-changes-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&dir) {
                Ok(()) => return Self(dir),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }

    fn put(&self, path: &str, bytes: &str) {
        let file = self.0.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn invoke(cwd: &Path, args: &[&str], stdin: Option<&Value>) -> (bool, Value) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ff"))
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    if let Some(input) = stdin {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    let json = serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{args:?}: {e}; stdout={}; stderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    });
    (output.status.success(), json)
}

fn checkpoint(root: &Tree, extra: &[&str]) -> Value {
    let mut args = vec!["checkpoint", "--experimental"];
    args.extend(extra);
    let (ok, data) = invoke(&root.0, &args, None);
    assert!(ok, "{data}");
    assert!(data["checkpoint"].is_object(), "{data}");
    data["checkpoint"].clone()
}

fn changes(root: &Tree, state: &Value, extra: &[&str]) -> (bool, Value) {
    let mut args = vec!["changes", "--experimental", "--since", "-"];
    args.extend(extra);
    invoke(&root.0, &args, Some(state))
}

#[test]
fn endpoint_delta_detects_same_size_same_mtime_and_does_not_guess_renames() {
    let tree = Tree::new();
    tree.put("a.txt", "before");
    tree.put("b.txt", "rename me");
    let path = tree.0.join("a.txt");
    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
    let state = checkpoint(&tree, &[]);
    tree.put("a.txt", "after!");
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), mtime);
    fs::rename(tree.0.join("b.txt"), tree.0.join("c.txt")).unwrap();
    let (ok, out) = changes(&tree, &state, &[]);
    assert!(ok, "{out}");
    assert_eq!(out["status"], "observed_changes");
    assert_eq!(
        out["events"],
        serde_json::json!([
            {"kind": "modified", "path": "a.txt"},
            {"kind": "deleted", "path": "b.txt"},
            {"kind": "added", "path": "c.txt"}
        ])
    );
    assert!(!out.to_string().contains("before"));
}

#[test]
fn no_observed_changes_explicitly_excludes_transient_history() {
    let tree = Tree::new();
    let state = checkpoint(&tree, &[]);
    tree.put("transient.txt", "gone before comparison");
    fs::remove_file(tree.0.join("transient.txt")).unwrap();
    let (ok, out) = changes(&tree, &state, &[]);
    assert!(ok, "{out}");
    assert_eq!(out["status"], "no_observed_changes");
    assert_eq!(out["events"], serde_json::json!([]));
    assert!(out["limitations"]
        .as_array()
        .unwrap()
        .contains(&Value::from("transient_changes_not_observable")));
    assert_eq!(fs::read_dir(&tree.0).unwrap().count(), 0);
}

fn assert_recreate((ok, out): (bool, Value), reason: &str) {
    assert!(ok, "{out}");
    assert_eq!(out["status"], "recreate_checkpoint", "{out}");
    assert_eq!(out["reason"], reason);
    assert_eq!(out["action"], "run_checkpoint_again");
    assert!(out.get("events").is_none());
}

#[test]
fn state_is_bound_to_root_and_checkout_marker() {
    let tree = Tree::new();
    let other = Tree::new();
    tree.put(".git", "gitdir: ../missing/worktrees/one\n");
    other.put(".git", "gitdir: ../missing/worktrees/one\n");
    let state = checkpoint(&tree, &[]);
    assert_recreate(changes(&other, &state, &[]), "root_or_checkout_changed");
    tree.put(".git", "gitdir: ../missing/worktrees/two\n");
    assert_recreate(changes(&tree, &state, &[]), "root_or_checkout_changed");
}

#[test]
fn expired_versioned_and_invalid_state_requires_recreation() {
    let tree = Tree::new();
    let state = checkpoint(&tree, &[]);
    let mut edited = state.clone();
    edited["issued_at"] = Value::from(0);
    assert_recreate(changes(&tree, &edited, &[]), "expired_checkpoint");
    edited = state.clone();
    edited["version"] = Value::from(999);
    assert_recreate(changes(&tree, &edited, &[]), "checkpoint_version");
    edited = state;
    edited["files"]["../outside"] = Value::from("a".repeat(64));
    assert_recreate(changes(&tree, &edited, &[]), "invalid_checkpoint");
    assert_recreate(
        changes(&tree, &Value::from("x".repeat(2 * 1024 * 1024)), &[]),
        "invalid_checkpoint",
    );
}

#[test]
fn ignore_and_bounded_artifact_opt_in_keep_sensitive_names_out() {
    let tree = Tree::new();
    tree.put(".gitignore", "target/\nprivate*\n/sub/*.txt\n");
    tree.put("sub/.ignore", "!keep.txt\n");
    tree.put("sub/keep.txt", "visible");
    tree.put("sub/skip.txt", "hidden");
    tree.put("target/result.bin", "old");
    tree.put("target/.env.local/file.txt", "synthetic secret");
    tree.put("target/token.json", "synthetic secret");
    tree.put("target/private-artifact", "in explicit override scope");
    tree.put("private-file", "not in override scope");
    tree.put("other/target/result.bin", "not in override scope");
    let baseline = checkpoint(&tree, &[]);
    let opted_in = checkpoint(&tree, &["--include-artifacts"]);
    let files = opted_in["files"].as_object().unwrap();
    assert!(!baseline["files"]
        .as_object()
        .unwrap()
        .contains_key("target/result.bin"));
    for visible in [
        "sub/keep.txt",
        "target/result.bin",
        "target/private-artifact",
    ] {
        assert!(files.contains_key(visible), "missing {visible}");
    }
    for hidden in [
        "sub/skip.txt",
        "target/.env.local/file.txt",
        "target/token.json",
        "private-file",
        "other/target/result.bin",
    ] {
        assert!(!files.contains_key(hidden), "leaked {hidden}");
    }
    assert!(!opted_in.to_string().contains("synthetic secret"));
    assert_recreate(changes(&tree, &opted_in, &[]), "filter_policy_changed");
    tree.put("target/result.bin", "new");
    let (_, out) = changes(&tree, &opted_in, &["--include-artifacts"]);
    assert_eq!(
        out["events"],
        serde_json::json!([{"kind":"modified", "path":"target/result.bin"}])
    );
    tree.put("sub/.ignore", "# revised policy\n");
    assert_recreate(changes(&tree, &baseline, &[]), "filter_policy_changed");
}

#[test]
fn deletion_paths_are_openable_from_a_different_client_cwd() {
    let tree = Tree::new();
    tree.put("sub/a.txt", "old");
    let state = checkpoint(&tree, &[]);
    fs::remove_file(tree.0.join("sub/a.txt")).unwrap();
    let root = tree.0.to_str().unwrap();
    let (ok, out) = invoke(
        &tree.0.join("sub"),
        &["changes", "--experimental", "--root", root, "--since", "-"],
        Some(&state),
    );
    assert!(ok, "{out}");
    assert_eq!(out["events"][0]["path"], "a.txt");
}

#[test]
fn cancellation_while_waiting_for_input_emits_no_delta() {
    let tree = Tree::new();
    let mut child = Command::new(env!("CARGO_BIN_EXE_ff"))
        .current_dir(&tree.0)
        .args(["changes", "--experimental", "--since", "-", "--timeout=0.1"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let _hold_input_open = child.stdin.take().unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let out: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(out["error"]["code"], "TIMEOUT");
    assert!(out.get("events").is_none());
    assert!(out.get("checkpoint").is_none());
}

#[test]
fn byte_budget_is_indeterminate_not_unchanged_or_deleted() {
    let tree = Tree::new();
    tree.put("large.txt", "short");
    let state = checkpoint(&tree, &[]);
    fs::OpenOptions::new()
        .write(true)
        .open(tree.0.join("large.txt"))
        .unwrap()
        .set_len(256 * 1024 * 1024 + 1)
        .unwrap();
    assert_indeterminate(changes(&tree, &state, &[]), "byte_budget");
}

fn assert_indeterminate((ok, out): (bool, Value), reason: &str) {
    assert!(ok, "{out}");
    assert_eq!(out["status"], "indeterminate", "{out}");
    assert_eq!(out["coverage"], "incomplete");
    assert_eq!(out["reason"], reason);
    assert!(out.get("events").is_none());
    assert!(out.get("checkpoint").is_none());
}

#[test]
fn inaccessible_file_is_not_a_deletion() {
    let tree = Tree::new();
    tree.put("access.txt", "first");
    let state = checkpoint(&tree, &[]);
    let path = tree.0.join("access.txt");
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let _locked = fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(fs::File::open(&path).is_err());
        assert_indeterminate(changes(&tree, &state, &[]), "access_denied");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0)).unwrap();
        // Root (including some container runners) can bypass mode-bit denial.
        if let Err(error) = fs::File::open(&path) {
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
            assert_indeterminate(changes(&tree, &state, &[]), "access_denied");
        }
    }
}

#[test]
fn external_link_is_not_followed_and_makes_coverage_incomplete() {
    let tree = Tree::new();
    let outside = Tree::new();
    outside.put("external-name.txt", "outside");
    let state = checkpoint(&tree, &[]);
    let link = tree.0.join("linked");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside.0, &link).unwrap();
    #[cfg(windows)]
    {
        let output = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/j"])
            .arg(&link)
            .arg(&outside.0)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
    }
    assert!(link.join("external-name.txt").is_file());
    let result = changes(&tree, &state, &[]);
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    assert!(!result.1.to_string().contains("external-name"));
    assert_indeterminate(result, "link_or_reparse_point");
}

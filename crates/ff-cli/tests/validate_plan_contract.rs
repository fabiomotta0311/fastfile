use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Repo(PathBuf);
impl Repo {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ff-plan-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let repo = Self(dir);
        repo.git(&["init", "-q"]);
        repo.git(&["config", "user.name", "FastFile Fixture"]);
        repo.git(&["config", "user.email", "fixture@example.invalid"]);
        repo.git(&["config", "core.autocrlf", "false"]);
        repo
    }
    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}: {out:?}");
    }
    fn write(&self, path: &str, content: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    fn commit(&self) {
        self.git(&["add", "."]);
        self.git(&["commit", "-qm", "fixture"]);
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_ff"))
            .current_dir(&self.0)
            .arg("validate-plan")
            .args(args)
            .output()
            .unwrap()
    }
    fn plan(&self, args: &[&str]) -> Value {
        let mut options = vec!["--experimental"];
        options.extend_from_slice(args);
        let out = self.run(&options);
        assert!(out.status.success(), "{out:?}");
        let plan: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(plan["executed"], false);
        plan
    }
}
impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn separates_layers_and_finds_test_in_another_module() {
    let repo = Repo::new();
    repo.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    1\n}\n");
    repo.write(
        "tests/check.rs",
        "#[test]\nfn checks_answer() { assert_eq!(sample::answer(), 3); }\n",
    );
    repo.write(".gitignore", "ignored.rs\n");
    repo.commit();
    assert!(!repo.run(&[]).status.success());
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    2\n}\n");
    repo.git(&["add", "src/lib.rs"]);
    repo.write("src/lib.rs", "pub fn answer() -> u8 {\n    3\n}\n");
    repo.write("new.rs", "fn local_file() {}\n");
    repo.write("ignored.rs", "fn ignored() {}\n");
    repo.write(".env", "SESSION=sensitive_marker\n");
    let plan = repo.plan(&["--path", "src"]);
    let changes = plan["changes"].as_array().unwrap();
    assert_eq!(changes.len(), 2, "{plan}");
    assert_eq!(changes[0]["layer"], "staged");
    assert_eq!(changes[1]["layer"], "unstaged");
    assert_eq!(changes[1]["symbols"][0], "answer");
    assert_eq!(
        plan["suggestions"][0]["argv"],
        serde_json::json!([
            "cargo",
            "test",
            "-p",
            "sample",
            "--test",
            "check",
            "checks_answer",
            "--locked"
        ])
    );
    assert_eq!(
        plan["suggestions"][0]["evidence"][0]["path"],
        "tests/check.rs"
    );
    let full = repo.plan(&["--base", "HEAD"]);
    assert_eq!(full["changes"].as_array().unwrap().len(), 3, "{full}");
    assert_eq!(full["changes"][2]["layer"], "untracked");
    assert!(!full.to_string().contains("sensitive_marker"));
    assert!(!full.to_string().contains(".env"));
    assert!(!full.to_string().contains("ignored.rs"));
}

#[test]
fn rename_removal_dependents_and_config_fallback() {
    let repo = Repo::new();
    repo.write("Cargo.toml", "[workspace]\nmembers = ['core', 'app']\n");
    repo.write(
        "core/Cargo.toml",
        "[package]\nname = 'sample-core'\nversion = '0.1.0'\n",
    );
    repo.write("core/src/lib.rs", "mod removed;\n");
    repo.write("core/src/removed.rs", "fn gone() {}\n");
    repo.write("core/src/old.rs", "fn moved() {}\n");
    repo.write("app/Cargo.toml", "[package]\nname = 'sample-app'\nversion = '0.1.0'\n[dependencies]\nalias = { path = '../core', package = 'sample-core' }\n");
    repo.write("app/src/lib.rs", "pub fn demo() {}\n");
    repo.write(".github/workflows/ci.yml", "jobs:\n  checks:\n    steps:\n      - run: cargo test --workspace --locked\n      - run: echo never-run-this\n");
    repo.commit();
    repo.git(&["mv", "core/src/old.rs", "core/src/new.rs"]);
    repo.git(&["rm", "core/src/removed.rs"]);
    let plan = repo.plan(&[]);
    let changes = plan["changes"].as_array().unwrap();
    assert!(
        changes.iter().any(|c| c["status"] == "renamed"
            && c["old_path"] == "core/src/old.rs"
            && c["path"] == "core/src/new.rs"),
        "{plan}"
    );
    assert!(changes
        .iter()
        .any(|c| c["status"] == "deleted" && c["path"] == "core/src/removed.rs"));
    assert_eq!(
        plan["impacted_packages"],
        serde_json::json!(["sample-app", "sample-core"])
    );
    repo.write(".cargo/config.toml", "[build]\nincremental = false\n");
    let fallback = repo.plan(&[]);
    assert_eq!(
        fallback["suggestions"][0]["argv"],
        serde_json::json!(["cargo", "test", "--workspace", "--locked"])
    );
    assert_eq!(
        fallback["suggestions"][0]["evidence"][0]["path"],
        ".github/workflows/ci.yml"
    );
    assert!(!fallback.to_string().contains("never-run-this"));
}

#[test]
fn explicit_base_keeps_committed_and_worktree_changes_distinct() {
    let repo = Repo::new();
    repo.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    repo.write("src/lib.rs", "pub fn initial() {}\n");
    repo.commit();
    repo.git(&["tag", "validation-base"]);
    repo.write("src/lib.rs", "pub fn committed() {}\n");
    repo.commit();
    repo.write("src/lib.rs", "pub fn working() {}\n");
    let plan = repo.plan(&["--base", "validation-base"]);
    assert_eq!(plan["coverage"]["base_to_index"], "base_to_index");
    assert_eq!(plan["changes"][0]["layer"], "base_to_index");
    assert_eq!(plan["changes"][1]["layer"], "unstaged");
    let default = repo.plan(&[]);
    assert_ne!(plan["base_commit"], default["base_commit"]);
    assert_eq!(default["changes"].as_array().unwrap().len(), 1);
    assert!(!repo
        .run(&["--experimental", "--path", "../outside"])
        .status
        .success());
    assert!(!repo
        .run(&["--experimental", "--base", "HEAD~1"])
        .status
        .success());
}

#[test]
fn configured_git_helpers_are_not_called_and_the_index_is_unchanged() {
    let repo = Repo::new();
    repo.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    repo.write(".gitattributes", "*.rs diff=fixture filter=fixture\n");
    repo.write("src/lib.rs", "pub fn original() {}\n");
    repo.commit();
    for key in [
        "core.fsmonitor",
        "diff.fixture.command",
        "diff.fixture.textconv",
        "filter.fixture.clean",
    ] {
        repo.git(&["config", key, "echo unexpected > planner-helper-marker"]);
    }
    repo.write("src/lib.rs", "pub fn changed() {}\n");
    let index = repo.0.join(".git/index");
    let before = fs::read(&index).unwrap();
    let plan = repo.plan(&[]);
    assert!(
        plan["changes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|change| change["layer"] == "unstaged"
                && change["symbols"]
                    .as_array()
                    .unwrap()
                    .contains(&Value::from("changed"))),
        "{plan}"
    );
    assert_eq!(fs::read(index).unwrap(), before);
    assert!(!repo.0.join("planner-helper-marker").exists());
}

#[test]
fn oversized_worktree_file_is_not_reported_as_clean() {
    let repo = Repo::new();
    repo.write(
        "Cargo.toml",
        "[package]\nname = 'sample'\nversion = '0.1.0'\n",
    );
    repo.write("src/lib.rs", "pub fn original() {}\n");
    repo.commit();
    repo.write("src/lib.rs", &"x".repeat(130 * 1024));
    let plan = repo.plan(&[]);
    assert_eq!(plan["coverage"]["complete"], false);
    assert_eq!(plan["fallback_reasons"][0], "partial_diff");
    assert_eq!(plan["suggestions"][0]["scope"], "workspace");
}

use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ff-context-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        #[cfg(not(windows))]
        let dir = dir.canonicalize().unwrap();
        let f = Self(dir);
        f.write(
            "repo/src/math.rs",
            "pub fn total() -> usize {\n    42\n}\n\n#[test]\nfn adjacent() {}\n",
        );
        f.write("repo/tests/amount.rs", "#[test]\nfn checks_total() {\n    // total is a textual mention, not a call.\n    assert_eq!(example::total(), 42);\n}\n");
        f
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn cli(&self, args: &[&str]) -> (bool, Value) {
        let out = Command::new(env!("CARGO_BIN_EXE_ff"))
            .current_dir(&self.0)
            .args(["context", "--root", "repo"])
            .args(args)
            .output()
            .unwrap();
        let json = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&out.stderr)));
        (out.status.success(), json)
    }
    fn run(&self, args: &[&str]) -> Value {
        let (success, value) = self.cli(args);
        assert!(success, "{value}");
        value
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snippets(report: &Value) -> &[Value] {
    report["snippets"].as_array().unwrap()
}
fn evidence(snippet: &Value) -> &[Value] {
    snippet["evidence"].as_array().unwrap()
}

#[test]
fn location_is_verified_against_a_real_rust_file() {
    let f = Fixture::new();
    let file = f.0.join("repo/src/math.rs");
    #[cfg(windows)]
    assert_eq!(file.display().to_string().as_bytes()[1], b':');
    let result = f.run(&["--location", &format!("{}:2", file.display())]);
    assert_eq!(result["anchor"]["status"], "observed");
    assert_eq!(result["anchor"]["line"], 2);
    assert_eq!(result["symbol"], "total");
    let path = result["anchor"]["path"].as_str().unwrap();
    assert_eq!(
        f.0.join(path).canonicalize().unwrap(),
        file.canonicalize().unwrap()
    );
    assert!(snippets(&result)
        .iter()
        .flat_map(evidence)
        .any(|e| e["kind"] == "test"
            && e["reasons"]
                .as_array()
                .unwrap()
                .contains(&Value::from("same_module"))));
}

#[test]
fn diagnostic_finds_cross_file_test_and_merges_overlapping_ranges() {
    let f = Fixture::new();
    f.write("repo/unrelated.rs", "fn unrelated() {} // NOT_SELECTED\n");
    let args = [
        "--diagnostic",
        "repo/src/math.rs:2|password=fixtureDiagnosisNeverEcho",
    ];
    let result = f.run(&args);
    assert_eq!(result, f.run(&args));
    assert_eq!(result["anchor"]["kind"], "diagnostic");
    assert_eq!(result["anchor"]["status"], "observed");
    let cross: Vec<_> = snippets(&result)
        .iter()
        .filter(|s| s["path"] == "repo/tests/amount.rs")
        .collect();
    assert_eq!(cross.len(), 1);
    assert_eq!(cross[0]["start_line"], 1);
    assert_eq!(cross[0]["end_line"], 5);
    let hits = evidence(cross[0]);
    assert!(hits.iter().any(|e| e["kind"] == "test"
        && e["status"] == "candidate"
        && e["line"] == 2
        && e["reasons"][0] == "textual_reference"));
    assert_eq!(
        hits.iter()
            .filter(|e| e["kind"] == "textual_reference" && e["status"] == "candidate")
            .count(),
        2
    );
    assert!(!result.to_string().contains("fixtureDiagnosisNeverEcho"));
    assert!(!result.to_string().contains("NOT_SELECTED"));
}

#[test]
fn symbol_homonyms_are_not_resolved() {
    let f = Fixture::new();
    f.write("repo/src/other.rs", "fn total() {}\nfn total_approx() {}\n");
    let result = f.run(&["--symbol", "total"]);
    assert_eq!(result["symbol_status"], "ambiguous");
    assert_eq!(result["definition_count"], 2);
    let hits: Vec<_> = snippets(&result)
        .iter()
        .flat_map(evidence)
        .filter(|e| e["kind"] == "definition" && e["status"] == "observed")
        .collect();
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().all(|e| e["line"] == 1));
}

#[test]
fn scope_and_secret_policy_apply_even_to_explicit_anchors() {
    let f = Fixture::new();
    f.write("repo/.gitignore", "ignored.rs\n");
    for path in [
        "outside.rs",
        "repo/ignored.rs",
        "repo/target/generated.rs",
        "repo/.env.vault/code.rs",
    ] {
        f.write(path, "fn total() {} // NEVER_EXPOSE\n");
        let result = f.run(&["--location", &format!("{path}:1")]);
        assert_eq!(result["anchor"]["status"], "unverified", "{path}: {result}");
        assert!(snippets(&result).is_empty());
    }
    f.write(
        "repo/src/math.rs",
        "fn total() {\n    let password = \"fixturePasswordNeverEcho\";\n}\n",
    );
    let result = f.run(&["--symbol", "total"]);
    assert_eq!(result["definition_count"], 1);
    assert!(!result.to_string().contains("fixturePasswordNeverEcho"));
    assert!(result.to_string().contains("redacted:assignment"));
    assert_eq!(
        f.run(&["--location", "repo/src/math.rs:900"])["anchor"]["status"],
        "unverified"
    );
}

#[test]
fn oversized_source_is_partial_not_absent() {
    let f = Fixture::new();
    f.write("repo/large.rs", &" ".repeat(256 * 1024 + 1));
    let result = f.run(&["--symbol", "unseen"]);
    assert_eq!(result["coverage"], "partial");
    assert_eq!(result["symbol_status"], "partial");
    assert!(result["warnings"]
        .as_array()
        .unwrap()
        .contains(&Value::from("file_size_limit")));
}

#[test]
fn malformed_or_conflicting_inputs_fail() {
    let f = Fixture::new();
    for args in [
        &[][..],
        &["--location", "repo/src/math.rs:0"],
        &["--diagnostic", "no delimiter"],
        &["--symbol", "total", "--location", "repo/src/math.rs:1"],
    ] {
        assert!(!f.cli(args).0);
    }
}

#[test]
fn directory_links_do_not_extend_the_root() {
    let f = Fixture::new();
    f.write("outer/other.rs", "fn total() {} // NEVER_EXPOSE\n");
    let link = f.0.join("repo/link");
    let outer = f.0.join("outer");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outer, &link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let output = Command::new("cmd.exe")
            .args(["/d", "/v:off", "/c"])
            .env("FF_CONTEXT_LINK", &link)
            .env("FF_CONTEXT_TARGET", &outer)
            .raw_arg(r#"mklink /J "%FF_CONTEXT_LINK%" "%FF_CONTEXT_TARGET%""#)
            .output()
            .unwrap();
        assert!(output.status.success(), "{:?}", output);
    }
    let result = f.run(&["--symbol", "total"]);
    assert_eq!(result["definition_count"], 1);
    assert!(!result.to_string().contains("NEVER_EXPOSE"));
    assert_eq!(
        f.run(&["--location", "repo/link/other.rs:1"])["anchor"]["status"],
        "unverified"
    );
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
}

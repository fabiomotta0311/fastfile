use std::{fs, process::Command};

#[test]
fn truncated_json_respects_budget_for_grep_diff_and_dupes() {
    let dir = std::env::temp_dir().join(format!("ff-budget-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.txt"), format!("needle {}\n", "x".repeat(3000))).unwrap();
    fs::write(dir.join("b.txt"), format!("needle {}\n", "y".repeat(3000))).unwrap();
    fs::write(dir.join("c.txt"), format!("needle {}\n", "x".repeat(3000))).unwrap();
    let cases: &[&[&str]] = &[
        &["grep", "needle", "."],
        &["diff", "a.txt", "b.txt"],
        &["dupes", "."],
    ];
    for args in cases {
        let mut found = false;
        for budget in (120..350).step_by(5) {
            let out = Command::new(env!("CARGO_BIN_EXE_ff"))
                .current_dir(&dir)
                .args(*args)
                .args(["--json", "--compact", "--budget", &budget.to_string()])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{:?}: {}",
                args,
                String::from_utf8_lossy(&out.stderr)
            );
            let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
            let text = String::from_utf8(out.stdout).unwrap();
            if value["truncated"] == true && !text.contains("insuficiente") {
                found = true;
                assert!(
                    text.trim_end().len() <= budget * 4,
                    "{:?} budget {}: {} bytes: {}",
                    args,
                    budget,
                    text.trim_end().len(),
                    text
                );
            }
        }
        assert!(found, "no truncation case exercised for {:?}", args);
    }
    fs::remove_dir_all(dir).unwrap();
}

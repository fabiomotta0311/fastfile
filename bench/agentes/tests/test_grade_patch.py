import json
import os
import subprocess
import sys
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
from difflib import unified_diff
from pathlib import Path
from unittest.mock import patch

BENCH = Path(__file__).resolve().parents[1]
SCRIPT = BENCH / "grade_patch.py"
sys.path.insert(0, str(BENCH))
import corpus  # noqa: E402
import grade_patch  # noqa: E402

IMAGE = os.environ.get("FF_BENCH_DOCKER_IMAGE")
MISSING_IMAGE = "sha256:" + "0" * 64
PATH = "crates/ws-text/src/words.rs"
OLD = '    s.trim().to_string()'
FIX = '    s.split_whitespace().collect::<Vec<_>>().join(" ")'
ALTERNATIVE = '    s.split_whitespace().collect::<Vec<&str>>().join(" ")'


def change(path, old, new):
    header = f"diff --git a/{path} b/{path}\n--- a/{path}\n+++ b/{path}\n"
    if path != PATH:
        return header + f"@@ -3 +3 @@\n-{old}\n+{new}\n"
    lines = new.splitlines()
    return (header + f"@@ -2,3 +2,{len(lines) + 2} @@\n"
            " pub fn normalize(s: &str) -> String {\n" + f"-{old}\n"
            + "".join(f"+{line}\n" for line in lines) + " }\n")


class GradePatch(unittest.TestCase):
    def grade(self, content, code, reason, image=MISSING_IMAGE, seconds=60, task="P01"):
        with tempfile.TemporaryDirectory() as d:
            patch = Path(d) / "candidate.diff"
            patch.write_bytes(content.encode())
            proc = subprocess.run(
                [sys.executable, str(SCRIPT), task, str(patch), "--image", image,
                 "--timeout", str(seconds)], cwd=d, capture_output=True, text=True, timeout=220,
            )
            self.assertEqual(proc.stderr, "")
            self.assertEqual(proc.returncode, code, proc.stdout)
            verdict = json.loads(proc.stdout)
            self.assertEqual(verdict["reason"], reason, verdict)
            self.assertEqual(verdict["exit_code"], code)
            self.assertEqual(verdict["task"], task)
            self.assertEqual(verdict["sandbox_image"], image)
            self.assertEqual(len(verdict["patch_sha256"]), 64)
            self.assertNotIn(OLD.strip(), proc.stdout)
            self.assertEqual(patch.read_text(), content)
            self.assertEqual(list(Path(d).iterdir()), [patch])
            return verdict

    def test_empty_rejected_without_executor(self):
        self.grade("", 1, "empty_patch")

    def test_inapplicable_rejected_without_executor(self):
        self.grade(change(PATH, FIX, ALTERNATIVE), 1, "patch_does_not_apply")

    def test_evaluator_test_addition_rejected_without_executor(self):
        path = "crates/ws-text/tests/disc_p01.rs"
        patch = (f"diff --git a/{path} b/{path}\nnew file mode 100644\n"
                 f"--- /dev/null\n+++ b/{path}\n@@ -0,0 +1 @@\n+#[test] fn disc_p01() {{}}\n")
        self.grade(patch, 1, "scope_violation")

    def test_mixed_patch_cannot_change_preserved_files(self):
        patch = change(PATH, OLD, FIX) + change("Cargo.toml", 'resolver = "2"', 'resolver = "1"')
        self.grade(patch, 1, "scope_violation")

    def test_inline_regression_assertion_cannot_be_edited(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "fixture"
            corpus.materializar(corpus.carregar_correcoes()[0], root)
            before = (root / PATH).read_text()
        after = before.replace(OLD, FIX).replace('assert_eq!(normalize("a b"), "a b");', '')
        patch = "".join(unified_diff(before.splitlines(True), after.splitlines(True), f"a/{PATH}", f"b/{PATH}"))
        self.grade(patch, 1, "preserved_tests_modified")

    @unittest.skipUnless(IMAGE, "defina FF_BENCH_DOCKER_IMAGE (imagem local confiável)")
    def test_missing_sandbox_fails_closed(self):
        verdict = self.grade(change(PATH, OLD, ALTERNATIVE), 3, "sandbox_image_unavailable")
        self.assertEqual(verdict["phase"], "baseline")
        self.assertEqual(verdict["runs"], {})

    @unittest.skipUnless(IMAGE, "defina FF_BENCH_DOCKER_IMAGE (imagem local confiável)")
    def test_alternative_patch_accepted_with_real_red_and_green(self):
        verdict = self.grade(change(PATH, OLD, ALTERNATIVE), 0, "tests_passed", IMAGE)
        runs = verdict["runs"]
        self.assertEqual(runs["baseline"]["failed"], 1)
        self.assertGreater(runs["baseline"]["passed"], 0)
        self.assertEqual(runs["reference"], runs["candidate"])
        self.assertEqual(runs["candidate"]["failed"], 0)
        self.assertEqual(runs["candidate"]["passed"], runs["baseline"]["passed"] + 1)

    @unittest.skipUnless(IMAGE, "defina FF_BENCH_DOCKER_IMAGE (imagem local confiável)")
    def test_regression_rejected_even_when_discriminant_passes(self):
        patch = change(PATH, OLD, '    if s == "a b" { String::new() } else { '
                       + FIX.strip() + ' }')
        verdict = self.grade(patch, 1, "candidate_tests_failed", IMAGE)
        self.assertEqual(verdict["runs"]["candidate"]["failed"], 1)
        self.assertTrue(verdict["runs"]["candidate"]["discriminant_passed"])

    @unittest.skipUnless(IMAGE, "defina FF_BENCH_DOCKER_IMAGE (imagem local confiável)")
    def test_timeout_is_distinct_and_container_is_removed(self):
        patch = change(PATH, OLD, '    std::thread::sleep(std::time::Duration::from_secs(120));\n'
                       '    ' + FIX.strip())
        verdict = self.grade(patch, 2, "sandbox_timeout", IMAGE, seconds=15)
        self.assertEqual(verdict["phase"], "candidate")
        result = subprocess.check_output(["docker", "ps", "-aq", "--filter", "label=fastfile.cpst.grade"], text=True)
        self.assertFalse(result.strip())

    @unittest.skipUnless(IMAGE, "defina FF_BENCH_DOCKER_IMAGE (imagem local confiável)")
    def test_isolation_boundary_with_real_container(self):
        task = corpus.carregar_correcoes()[0]
        with tempfile.TemporaryDirectory() as d, patch.dict("os.environ", {"FF_TEST_SECRET": "synthetic"}):
            scratch = Path(d)
            tree = scratch / "baseline"
            corpus.materializar(task, tree)
            grade_patch.stage_oracle(tree, task)
            rc, _ = grade_patch.run(grade_patch.local_docker(scratch), tree, IMAGE, ["/bin/sh", "-ec", """
                test "$(id -u)" = 65534
                test -z "${FF_TEST_SECRET+x}"
                test ! -e /fixture/config/.env
                test ! -e /var/run/docker.sock
                test "$(ls /sys/class/net)" = lo
                test -f /fixture/crates/ws-text/tests/disc_p01.rs
                if touch /fixture/crates/ws-text/tests/disc_p01.rs; then exit 1; fi
                touch /tmp/scratch-file
            """], 15)
            self.assertEqual(rc, 0)

    @unittest.skipUnless(IMAGE, "defina FF_BENCH_DOCKER_IMAGE (imagem local confiável)")
    def test_all_frozen_tasks_discriminate_under_isolation(self):
        def check(task):
            with tempfile.TemporaryDirectory() as d:
                root = Path(d)
                baseline, reference = root / "baseline", root / "reference"
                corpus.materializar(task, baseline)
                corpus.materializar({**task, "mutacoes": []}, reference)
                patch = ""
                for path in task["escopo_esperado"]:
                    before = (baseline / path).read_text(encoding="utf-8").splitlines(True)
                    after = (reference / path).read_text(encoding="utf-8").splitlines(True)
                    patch += "".join(unified_diff(before, after, f"a/{path}", f"b/{path}"))
                return self.grade(patch, 0, "tests_passed", IMAGE, task=task["id"])

        with ThreadPoolExecutor(max_workers=2) as pool:
            verdicts = list(pool.map(check, corpus.carregar_correcoes()))
        self.assertEqual(len(verdicts), 10)

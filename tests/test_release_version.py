"""Exercise the actual release version step without running a release."""
import os
from pathlib import Path
import subprocess
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = ROOT / '.github/workflows/release.yml'


def version_step():
    text = WORKFLOW.read_text(encoding='utf-8')
    step = text.split('      - id: ver\n', 1)[1].split('\n  build-release:', 1)[0]
    script = step.split('        run: |\n', 1)[1]
    # Expressions interpolated into shell source enable command injection.
    if '${{' in script:
        raise AssertionError('untrusted expressions must not be interpolated into shell')
    return '\n'.join(line[10:] for line in script.splitlines() if line.strip())


class ReleaseVersionTest(unittest.TestCase):
    def run_step(self, version, ref='refs/heads/main'):
        with tempfile.TemporaryDirectory() as temp:
            output = Path(temp, 'output')
            env = dict(os.environ, INPUT_VERSION=version, GITHUB_REF=ref,
                       GITHUB_OUTPUT=output.as_posix())
            result = subprocess.run([shutil.which('bash'), '--noprofile', '--norc', '-e', '-c', version_step()],
                                    cwd=ROOT, env=env, capture_output=True, text=True)
            return result, output.read_text() if output.exists() else ''

    def test_valid_manual_and_tag_versions(self):
        for version, ref in [('v1.2.3', 'refs/heads/main'), ('', 'refs/tags/v1.2.3')]:
            result, output = self.run_step(version, ref)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(output, 'version=1.2.3\ntag=v1.2.3\n')

    def test_untrusted_shell_and_newline_are_rejected(self):
        for version in ['1.2.3$(false)', '1.2.3\ninjected=value', '1.2.3"; exit 0; #']:
            result, output = self.run_step(version)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output, '')


if __name__ == '__main__':
    unittest.main()

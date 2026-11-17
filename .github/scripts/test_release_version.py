import os
from pathlib import Path
import subprocess
import tempfile
import textwrap
import unittest

ROOT = Path(__file__).resolve().parents[2]


class ReleaseVersionTest(unittest.TestCase):
    def run_version(self, version, ref):
        workflow = (ROOT / '.github/workflows/release.yml').read_text(encoding='utf-8')
        step = workflow.split('      - id: ver\n', 1)[1].split('      - ', 1)[0]
        block = step.split('        run: |\n', 1)[1]
        lines = []
        for line in block.splitlines():
            if line and not line.startswith('          '):
                break
            lines.append(line)
        script = textwrap.dedent('\n'.join(lines))
        with tempfile.TemporaryDirectory() as tmp:
            output = Path(tmp) / 'output'
            env = dict(os.environ, INPUT_VERSION=version, GITHUB_REF=ref, GITHUB_OUTPUT=str(output))
            result = subprocess.run([os.environ.get('TEST_BASH', 'bash'), '-e', '-c', script], cwd=ROOT, env=env, capture_output=True, text=True)
            return result, output.read_text() if output.exists() else ''

    def test_version_input_and_tag(self):
        result, output = self.run_version('v1.2.3-rc.1', 'refs/heads/main')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output, 'version=1.2.3-rc.1\ntag=v1.2.3-rc.1\n')
        result, output = self.run_version('', 'refs/tags/v2.3.4')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(output, 'version=2.3.4\ntag=v2.3.4\n')

    def test_untrusted_input_is_rejected_without_execution(self):
        result, output = self.run_version('1.2.3$(exit 42)', 'refs/heads/main')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Invalid release version', result.stderr)
        self.assertEqual(output, '')


if __name__ == '__main__':
    unittest.main()

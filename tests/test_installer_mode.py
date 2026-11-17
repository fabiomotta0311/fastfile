import subprocess
import unittest
from pathlib import Path


class InstallerModeTest(unittest.TestCase):
    def test_install_script_is_executable_in_git(self):
        root = Path(__file__).resolve().parents[1]
        entry = subprocess.check_output(
            ["git", "ls-files", "--stage", "--", "scripts/install.sh"],
            cwd=root,
            text=True,
        )
        self.assertTrue(entry.startswith("100755 "), entry)


if __name__ == "__main__":
    unittest.main()

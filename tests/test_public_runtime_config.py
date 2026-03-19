import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parent.parent
PRIVATE_CONFIGS = [
    ".mcp.json",
    ".codex/config.toml",
    ".codex/mcp-coordinator-upstream.json",
    ".claude/claude-desktop.config.mirror.json",
]


def git(*arguments):
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
    return subprocess.run(
        ["git", "-C", str(ROOT), "-c", f"safe.directory={ROOT}",
         "-c", f"core.excludesFile={os.devnull}", *arguments],
        env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )


class PublicRuntimeConfigTests(unittest.TestCase):
    def test_private_runtime_configs_have_repository_ignore_rules(self):
        for path in PRIVATE_CONFIGS:
            with self.subTest(path=path):
                result = git("check-ignore", "--no-index", "--verbose", path)
                self.assertEqual(result.returncode, 0, result.stderr or path)
                self.assertTrue(result.stdout.startswith(".gitignore:"), result.stdout)

    def test_portable_example_remains_tracked_and_available(self):
        self.assertEqual(git("ls-files", "--error-unmatch", "mcp.example.json").returncode, 0)
        self.assertEqual(git("check-ignore", "--no-index", "--quiet", "mcp.example.json").returncode, 1)

    def test_private_runtime_configs_are_not_tracked(self):
        self.assertEqual(git("ls-files", "--", *PRIVATE_CONFIGS).stdout.strip(), "")


if __name__ == "__main__":
    unittest.main()

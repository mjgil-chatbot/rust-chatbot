import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
FORGE_DOC = ROOT / "docs" / "forge-provider-integration.md"
VERIFY_DOC = ROOT / "docs" / "verification.md"


class ForgePlaintextDocTests(unittest.TestCase):
    def test_forge_provider_doc_explains_repeatable_plaintext_verification(self):
        content = FORGE_DOC.read_text()
        self.assertIn("### Streamed plaintext rendering contract", content)
        self.assertIn("Example verification command", content)
        self.assertIn("scripts/auto-ui-test-prompt-debug.sh", content)
        self.assertIn("last_markdown_row", content)

    def test_verification_doc_provides_portable_prompt_debug_command(self):
        content = VERIFY_DOC.read_text()
        self.assertIn("Example verification command", content)
        self.assertIn("--session-id <session-id>", content)
        self.assertIn("--require-substring 'fn main()'", content)
        self.assertIn("last_markdown_row", content)

    def test_release_docs_do_not_depend_on_private_acceptance_artifacts(self):
        for path in [FORGE_DOC, VERIFY_DOC]:
            with self.subTest(path=path.name):
                content = path.read_text()
                self.assertNotRegex(content, r"--session-id [0-9a-f]{8}-")
                self.assertNotIn("auto-ui/tmp/auto-ui-prompt-debug-", content)
                self.assertNotIn("That live run is considered the final acceptance artifact", content)


if __name__ == "__main__":
    unittest.main()

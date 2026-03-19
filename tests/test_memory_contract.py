import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_memory_contract.py"
REGRESSION_SCRIPT = ROOT / "scripts" / "verify-memory-regressions.sh"


def write_contract_docs(repo_root: Path) -> None:
    (repo_root / "docs" / "architecture").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs" / "architecture" / "memmachine-integration.md").write_text(
        textwrap.dedent(
            """
            Retrieved memory
            /memory-retrievals
            invalidated_doc_keys
            Current Native Enforcement
            scripts/verify_memory_contract.py
            scripts/verify-memory-regressions.sh
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "docs" / "verification.md").write_text(
        "## Memory Contract\nscripts/verify_memory_contract.py\nscripts/verify-memory-regressions.sh\nmemory contract\n",
        encoding="utf-8",
    )


def write_regression_sources(repo_root: Path) -> None:
    (repo_root / "src" / "memory_mcp").mkdir(parents=True, exist_ok=True)
    (repo_root / "src" / "memory_mcp" / "tests.rs").write_text(
        textwrap.dedent(
            """
            fn workspace_memory_status_routes_successfully() {}
            fn search_memory_routes_successfully() {}
            fn list_memory_injections_returns_persisted_user_turn_previews() {}
            fn list_memory_retrievals_returns_candidate_hits_and_injected_preview() {}
            fn invalidate_doc_key_routes_successfully() {}
            fn restore_doc_key_routes_successfully() {}
            fn remember_fact_rejects_invalid_priority() {}
            fn service_error_for_workspace_outside_allowed_roots() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )


def write_regression_script(repo_root: Path) -> None:
    (repo_root / "scripts").mkdir(parents=True, exist_ok=True)
    (repo_root / "scripts" / "verify-memory-regressions.sh").write_text(
        "\n".join(
            [
                "#!/usr/bin/env bash",
                "workspace_memory_status_routes_successfully",
                "search_memory_routes_successfully",
                "list_memory_injections_returns_persisted_user_turn_previews",
                "list_memory_retrievals_returns_candidate_hits_and_injected_preview",
                "invalidate_doc_key_routes_successfully",
                "restore_doc_key_routes_successfully",
                "remember_fact_rejects_invalid_priority",
                "service_error_for_workspace_outside_allowed_roots",
                "",
            ]
        ),
        encoding="utf-8",
    )


class MemoryContractTests(unittest.TestCase):
    def test_shell_script_parses(self) -> None:
        result = subprocess.run(
            ["bash", "-n", str(REGRESSION_SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_script_passes_with_complete_contract_fixture(self) -> None:
        with tempfile.TemporaryDirectory(prefix="memory-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)

            result = subprocess.run(
                ["python3", str(SCRIPT), "--repo-root", str(repo_root)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("memory-contract: PASS", result.stdout)

    def test_script_rejects_missing_required_regression_in_bundle(self) -> None:
        with tempfile.TemporaryDirectory(prefix="memory-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            regression_script = repo_root / "scripts" / "verify-memory-regressions.sh"
            regression_script.write_text(
                regression_script.read_text(encoding="utf-8").replace(
                    "restore_doc_key_routes_successfully\n",
                    "",
                ),
                encoding="utf-8",
            )

            result = subprocess.run(
                ["python3", str(SCRIPT), "--repo-root", str(repo_root)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing focused memory regression", result.stderr)

    def test_script_rejects_missing_required_regression(self) -> None:
        with tempfile.TemporaryDirectory(prefix="memory-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            tests_file = repo_root / "src" / "memory_mcp" / "tests.rs"
            tests_file.write_text(
                tests_file.read_text(encoding="utf-8").replace(
                    "fn service_error_for_workspace_outside_allowed_roots() {}\n",
                    "",
                ),
                encoding="utf-8",
            )

            result = subprocess.run(
                ["python3", str(SCRIPT), "--repo-root", str(repo_root)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing required memory regression", result.stderr)


if __name__ == "__main__":
    unittest.main()

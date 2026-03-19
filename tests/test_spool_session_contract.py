import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_spool_session_contract.py"
REGRESSION_SCRIPT = ROOT / "scripts" / "verify-spool-session-regressions.sh"


def write_contract_docs(repo_root: Path) -> None:
    (repo_root / "docs" / "architecture").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs" / "architecture" / "spool-session-contract-versioning.md").write_text(
        textwrap.dedent(
            """
            spool/session contract version = v1
            Compatible additive
            Behavioral compatible tightening
            Breaking contract
            scripts/verify_spool_session_contract.py
            scripts/verify-spool-session-regressions.sh
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "docs" / "architecture" / "worker-spool.md").write_text(
        textwrap.dedent(
            """
            schema_version = 1 on `request.json`
            schema_version = 1 on `state.json`
            schema_version = 1 on `result.json`
            Current Native Enforcement
            scripts/verify_spool_session_contract.py
            scripts/verify-spool-session-regressions.sh
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "docs" / "verification.md").write_text(
        "## Spool/Session Contract\nscripts/verify_spool_session_contract.py\nscripts/verify-spool-session-regressions.sh\nspool/session contract\n",
        encoding="utf-8",
    )


def write_regression_sources(repo_root: Path) -> None:
    (repo_root / "src" / "app" / "state").mkdir(parents=True, exist_ok=True)
    (repo_root / "src" / "session_mcp").mkdir(parents=True, exist_ok=True)
    (repo_root / "src" / "app" / "state" / "helpers_impl.rs").write_text(
        "fn read_worker_result_treats_unsupported_schema_version_as_malformed() {}\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "session_runtime_terminal_persistence_tests.rs").write_text(
        textwrap.dedent(
            """
            fn local_terminal_worker_persists_session_before_worker_cleanup() {}
            fn local_terminal_worker_keeps_existing_visible_assistant_when_terminal_response_is_think_only() {}
            fn remote_terminal_update_reloads_newer_disk_session_before_applying_response() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "sync_slots_tests.rs").write_text(
        textwrap.dedent(
            """
            fn poll_worker_slots_clears_processing_for_stale_request_only_spool_entry() {}
            fn poll_worker_slots_done_replaces_stale_provider_session_id() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "session_mcp" / "service.rs").write_text(
        "fn inspect_session_surfaces_worker_spool_truncation_metadata() {}\n",
        encoding="utf-8",
    )


def write_regression_script(repo_root: Path) -> None:
    (repo_root / "scripts").mkdir(parents=True, exist_ok=True)
    (repo_root / "scripts" / "verify-spool-session-regressions.sh").write_text(
        "\n".join(
            [
                "#!/usr/bin/env bash",
                "read_worker_result_treats_unsupported_schema_version_as_malformed",
                "local_terminal_worker_persists_session_before_worker_cleanup",
                "local_terminal_worker_keeps_existing_visible_assistant_when_terminal_response_is_think_only",
                "remote_terminal_update_reloads_newer_disk_session_before_applying_response",
                "poll_worker_slots_clears_processing_for_stale_request_only_spool_entry",
                "poll_worker_slots_done_replaces_stale_provider_session_id",
                "inspect_session_surfaces_worker_spool_truncation_metadata",
                "",
            ]
        ),
        encoding="utf-8",
    )


class SpoolSessionContractTests(unittest.TestCase):
    def test_shell_script_parses(self) -> None:
        result = subprocess.run(
            ["bash", "-n", str(REGRESSION_SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_script_passes_with_complete_contract_fixture(self) -> None:
        with tempfile.TemporaryDirectory(prefix="spool-session-contract-") as tmpdir:
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
            self.assertIn("spool-session-contract: PASS", result.stdout)

    def test_script_rejects_missing_required_regression_in_bundle(self) -> None:
        with tempfile.TemporaryDirectory(prefix="spool-session-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            regression_script = repo_root / "scripts" / "verify-spool-session-regressions.sh"
            regression_script.write_text(
                regression_script.read_text(encoding="utf-8").replace(
                    "inspect_session_surfaces_worker_spool_truncation_metadata\n",
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
            self.assertIn("missing focused spool/session regression", result.stderr)

    def test_script_rejects_missing_required_regression(self) -> None:
        with tempfile.TemporaryDirectory(prefix="spool-session-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            sync_slots = repo_root / "src" / "app" / "state" / "sync_slots_tests.rs"
            sync_slots.write_text(
                sync_slots.read_text(encoding="utf-8").replace(
                    "fn poll_worker_slots_done_replaces_stale_provider_session_id() {}\n",
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
            self.assertIn("missing required spool/session regression", result.stderr)


if __name__ == "__main__":
    unittest.main()

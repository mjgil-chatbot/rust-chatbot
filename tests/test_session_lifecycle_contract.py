import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_session_lifecycle_contract.py"
REGRESSION_SCRIPT = ROOT / "scripts" / "verify-session-lifecycle-regressions.sh"


def write_contract_docs(repo_root: Path) -> None:
    (repo_root / "docs" / "architecture").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs" / "architecture" / "session-worker-lifecycle-spec.md").write_text(
        textwrap.dedent(
            """
            ### 2. `PendingDraft`
            ### 6. `TerminalPendingReconcile`
            ### 8. `PrunedStartupGap`
            ### 9. `RecoveredError` / `RecoveredStopped`
            scripts/verify_session_lifecycle_contract.py
            scripts/verify-session-lifecycle-regressions.sh
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "docs" / "verification.md").write_text(
        "## Session/Worker Lifecycle Contract\nscripts/verify_session_lifecycle_contract.py\nscripts/verify-session-lifecycle-regressions.sh\nsession/worker lifecycle contract\n",
        encoding="utf-8",
    )


def write_regression_sources(repo_root: Path) -> None:
    (repo_root / "src" / "app" / "state").mkdir(parents=True, exist_ok=True)
    (repo_root / "src" / "app" / "state" / "ui_spec_lifecycle_tests.rs").write_text(
        "fn submit_valid_creates_pending_draft_and_requests_focus() {}\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "session_runtime_tests.rs").write_text(
        textwrap.dedent(
            """
            fn apply_stream_to_current_clears_pending_draft_on_first_chunk() {}
            fn local_terminal_worker_clears_pending_draft() {}
            fn local_terminal_worker_clears_pending_draft_even_on_empty_response() {}
            fn remote_terminal_update_clears_pending_draft() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "session_runtime_terminal_persistence_tests.rs").write_text(
        "fn local_terminal_worker_persists_session_before_worker_cleanup() {}\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "sync_slots_tests.rs").write_text(
        "fn poll_worker_slots_clears_processing_for_stale_request_only_spool_entry() {}\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "ui_io_tests.rs").write_text(
        "fn selected_session_reload_skips_replacement_when_pending_draft_active() {}\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "ui_io_reload_guard_tests.rs").write_text(
        textwrap.dedent(
            """
            fn selected_session_reload_drops_divergent_newer_snapshot_for_current_chat() {}
            fn selected_session_reload_accepts_strict_extension_even_with_equal_timestamp() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )


def write_regression_script(repo_root: Path) -> None:
    (repo_root / "scripts").mkdir(parents=True, exist_ok=True)
    (repo_root / "scripts" / "verify-session-lifecycle-regressions.sh").write_text(
        "\n".join(
            [
                "#!/usr/bin/env bash",
                "submit_valid_creates_pending_draft_and_requests_focus",
                "apply_stream_to_current_clears_pending_draft_on_first_chunk",
                "local_terminal_worker_clears_pending_draft",
                "local_terminal_worker_clears_pending_draft_even_on_empty_response",
                "remote_terminal_update_clears_pending_draft",
                "local_terminal_worker_persists_session_before_worker_cleanup",
                "poll_worker_slots_clears_processing_for_stale_request_only_spool_entry",
                "selected_session_reload_skips_replacement_when_pending_draft_active",
                "selected_session_reload_drops_divergent_newer_snapshot_for_current_chat",
                "selected_session_reload_accepts_strict_extension_even_with_equal_timestamp",
                "",
            ]
        ),
        encoding="utf-8",
    )


class SessionLifecycleContractTests(unittest.TestCase):
    def test_shell_script_parses(self) -> None:
        result = subprocess.run(
            ["bash", "-n", str(REGRESSION_SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_script_passes_with_complete_contract_fixture(self) -> None:
        with tempfile.TemporaryDirectory(prefix="session-lifecycle-contract-") as tmpdir:
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
            self.assertIn("session-lifecycle-contract: PASS", result.stdout)

    def test_script_rejects_missing_required_regression_in_bundle(self) -> None:
        with tempfile.TemporaryDirectory(prefix="session-lifecycle-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            regression_script = repo_root / "scripts" / "verify-session-lifecycle-regressions.sh"
            regression_script.write_text(
                regression_script.read_text(encoding="utf-8").replace(
                    "poll_worker_slots_clears_processing_for_stale_request_only_spool_entry\n",
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
            self.assertIn("missing focused lifecycle regression", result.stderr)

    def test_script_rejects_missing_required_regression(self) -> None:
        with tempfile.TemporaryDirectory(prefix="session-lifecycle-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            reload_guard = repo_root / "src" / "app" / "state" / "ui_io_reload_guard_tests.rs"
            reload_guard.write_text(
                reload_guard.read_text(encoding="utf-8").replace(
                    "fn selected_session_reload_accepts_strict_extension_even_with_equal_timestamp() {}\n",
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
            self.assertIn("missing required lifecycle regression", result.stderr)


if __name__ == "__main__":
    unittest.main()

import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_history_contract.py"
REGRESSION_SCRIPT = ROOT / "scripts" / "verify-history-regressions.sh"


def write_contract_docs(repo_root: Path) -> None:
    (repo_root / "docs" / "architecture").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs" / "architecture" / "chatbot-history-integration.md").write_text(
        textwrap.dedent(
            """
            byte-for-byte
            missing per-turn checkpoints
            Current Native Enforcement
            scripts/verify_history_contract.py
            scripts/verify-history-regressions.sh
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "docs" / "verification.md").write_text(
        "## History/Checkpoint Contract\nscripts/verify_history_contract.py\nscripts/verify-history-regressions.sh\nhistory/checkpoint contract\n",
        encoding="utf-8",
    )


def write_regression_sources(repo_root: Path) -> None:
    (repo_root / "src" / "app" / "state").mkdir(parents=True, exist_ok=True)
    (repo_root / "src" / "app" / "state" / "history_tests.rs").write_text(
        textwrap.dedent(
            """
            fn poll_history_sync_loads_missing_store_notice_via_background_refresh() {}
            fn drain_history_results_clears_stuck_refresh_on_disconnect() {}
            fn drain_history_results_clears_stuck_checkpoint_on_disconnect() {}
            fn load_history_snapshot_restores_deleted_store_byte_for_byte() {}
            fn build_checkpoint_manifest_skips_invalid_cargo_manifest_for_companion_discovery() {}
            fn build_checkpoint_manifest_ignores_unrelated_dirty_sibling_repo_patches() {}
            fn sync_chat_history_backfills_missing_auto_turn_checkpoints() {}
            fn load_history_snapshot_exposes_checkpoint_repo_summaries() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "history_repo_capture_tests.rs").write_text(
        textwrap.dedent(
            """
            fn build_checkpoint_manifest_omits_oversized_untracked_repo_patch() {}
            fn build_checkpoint_manifest_omits_repo_patch_when_remaining_quota_is_too_small() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )


def write_regression_script(repo_root: Path) -> None:
    (repo_root / "scripts").mkdir(parents=True, exist_ok=True)
    (repo_root / "scripts" / "verify-history-regressions.sh").write_text(
        "\n".join(
            [
                "#!/usr/bin/env bash",
                "poll_history_sync_loads_missing_store_notice_via_background_refresh",
                "drain_history_results_clears_stuck_refresh_on_disconnect",
                "drain_history_results_clears_stuck_checkpoint_on_disconnect",
                "load_history_snapshot_restores_deleted_store_byte_for_byte",
                "build_checkpoint_manifest_skips_invalid_cargo_manifest_for_companion_discovery",
                "build_checkpoint_manifest_ignores_unrelated_dirty_sibling_repo_patches",
                "build_checkpoint_manifest_omits_oversized_untracked_repo_patch",
                "build_checkpoint_manifest_omits_repo_patch_when_remaining_quota_is_too_small",
                "sync_chat_history_backfills_missing_auto_turn_checkpoints",
                "load_history_snapshot_exposes_checkpoint_repo_summaries",
                "",
            ]
        ),
        encoding="utf-8",
    )


class HistoryContractTests(unittest.TestCase):
    def test_shell_script_parses(self) -> None:
        result = subprocess.run(
            ["bash", "-n", str(REGRESSION_SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_script_passes_with_complete_contract_fixture(self) -> None:
        with tempfile.TemporaryDirectory(prefix="history-contract-") as tmpdir:
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
            self.assertIn("history-contract: PASS", result.stdout)

    def test_script_rejects_missing_required_regression_in_bundle(self) -> None:
        with tempfile.TemporaryDirectory(prefix="history-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            regression_script = repo_root / "scripts" / "verify-history-regressions.sh"
            regression_script.write_text(
                regression_script.read_text(encoding="utf-8").replace(
                    "load_history_snapshot_exposes_checkpoint_repo_summaries\n",
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
            self.assertIn("missing focused history regression", result.stderr)

    def test_script_rejects_missing_required_regression(self) -> None:
        with tempfile.TemporaryDirectory(prefix="history-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            history_tests = repo_root / "src" / "app" / "state" / "history_tests.rs"
            history_tests.write_text(
                history_tests.read_text(encoding="utf-8").replace(
                    "fn sync_chat_history_backfills_missing_auto_turn_checkpoints() {}\n",
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
            self.assertIn("missing required history regression", result.stderr)


if __name__ == "__main__":
    unittest.main()

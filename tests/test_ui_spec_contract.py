import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_ui_spec_contract.py"
REGRESSION_SCRIPT = ROOT / "scripts" / "verify-ui-spec-regressions.sh"


def write_contract_docs(repo_root: Path) -> None:
    (repo_root / "docs" / "architecture").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs").mkdir(parents=True, exist_ok=True)
    (repo_root / "docs" / "architecture" / "ui-lean-send-draft-stream-spec.md").write_text(
        textwrap.dedent(
            """
            ### Property 1: No Permanent Disabled-Composer State
            ### Property 2: Real Assistant Output Replaces Synthetic Draft Authority
            ### Property 3: Terminal Cleanup Clears Pending Draft Even With Empty Output
            ### Property 4: Passive Refresh Cannot Override Authoritative Draft State
            ### Property 5: Rendering Alone Does Not Mutate Draft Ownership
            scripts/verify_ui_spec_contract.py
            scripts/verify-ui-spec-regressions.sh
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "docs" / "architecture" / "ui-lean-integration-map.md").write_text(
        "scripts/verify_ui_spec_contract.py\nscripts/verify-ui-spec-regressions.sh\nsubmit_valid_creates_pending_draft_and_requests_focus\n",
        encoding="utf-8",
    )
    (repo_root / "docs" / "verification.md").write_text(
        "scripts/verify_ui_spec_contract.py\nscripts/verify-ui-spec-regressions.sh\nsend/draft/stream\n",
        encoding="utf-8",
    )


def write_regression_sources(repo_root: Path) -> None:
    (repo_root / "src" / "app" / "state").mkdir(parents=True, exist_ok=True)
    (repo_root / "src" / "app" / "ui" / "message_list" / "tests").mkdir(
        parents=True, exist_ok=True
    )
    (repo_root / "src" / "app" / "state" / "ui_spec_lifecycle_tests.rs").write_text(
        textwrap.dedent(
            """
            fn submit_valid_creates_pending_draft_and_requests_focus() {}
            fn local_terminal_empty_response_requests_focus_and_clears_pending_draft() {}
            fn send_failed_returns_chat_to_recoverable_idle_state() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "session_runtime_tests.rs").write_text(
        textwrap.dedent(
            """
            fn apply_stream_to_current_clears_pending_draft_on_first_chunk() {}
            fn apply_stream_to_current_empty_response_keeps_pending_draft() {}
            fn local_terminal_worker_clears_pending_draft() {}
            fn local_terminal_worker_clears_pending_draft_even_on_empty_response() {}
            fn remote_terminal_update_clears_pending_draft() {}
            fn remote_terminal_update_updates_current_session_owned_state() {}
            fn switching_chats_preserves_pending_draft_for_original_chat() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "state" / "ui_io_tests.rs").write_text(
        textwrap.dedent(
            """
            fn selected_session_reload_skips_replacement_when_pending_draft_active() {}
            fn selected_session_reload_proceeds_when_no_pending_draft() {}
            fn selected_session_reload_proceeds_when_pending_draft_is_for_other_chat() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (repo_root / "src" / "app" / "ui" / "message_list" / "tests" / "render_regressions.rs").write_text(
        textwrap.dedent(
            """
            fn render_with_pending_draft_does_not_clear_or_panic() {}
            fn render_with_pending_draft_for_other_chat_hides_indicator() {}
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )


def write_regression_script(repo_root: Path) -> None:
    (repo_root / "scripts").mkdir(parents=True, exist_ok=True)
    (repo_root / "scripts" / "verify-ui-spec-regressions.sh").write_text(
        "\n".join(
            [
                "#!/usr/bin/env bash",
                "submit_valid_creates_pending_draft_and_requests_focus",
                "apply_stream_to_current_clears_pending_draft_on_first_chunk",
                "apply_stream_to_current_empty_response_keeps_pending_draft",
                "local_terminal_worker_clears_pending_draft",
                "local_terminal_worker_clears_pending_draft_even_on_empty_response",
                "local_terminal_empty_response_requests_focus_and_clears_pending_draft",
                "remote_terminal_update_clears_pending_draft",
                "remote_terminal_update_updates_current_session_owned_state",
                "send_failed_returns_chat_to_recoverable_idle_state",
                "selected_session_reload_skips_replacement_when_pending_draft_active",
                "selected_session_reload_proceeds_when_no_pending_draft",
                "selected_session_reload_proceeds_when_pending_draft_is_for_other_chat",
                "switching_chats_preserves_pending_draft_for_original_chat",
                "render_with_pending_draft_does_not_clear_or_panic",
                "render_with_pending_draft_for_other_chat_hides_indicator",
                "",
            ]
        ),
        encoding="utf-8",
    )


class UiSpecContractTests(unittest.TestCase):
    def test_shell_script_parses(self) -> None:
        result = subprocess.run(
            ["bash", "-n", str(REGRESSION_SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_script_passes_with_complete_contract_fixture(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-spec-contract-") as tmpdir:
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
            self.assertIn("ui-spec-contract: PASS", result.stdout)

    def test_script_rejects_missing_required_regression_in_bundle(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-spec-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            regression_script = repo_root / "scripts" / "verify-ui-spec-regressions.sh"
            regression_script.write_text(
                regression_script.read_text(encoding="utf-8").replace(
                    "remote_terminal_update_clears_pending_draft\n", ""
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
            self.assertIn("missing focused regression", result.stderr)

    def test_script_rejects_missing_required_regression(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-spec-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            write_contract_docs(repo_root)
            write_regression_sources(repo_root)
            write_regression_script(repo_root)
            session_runtime = repo_root / "src" / "app" / "state" / "session_runtime_tests.rs"
            session_runtime.write_text(
                session_runtime.read_text(encoding="utf-8").replace(
                    "fn apply_stream_to_current_empty_response_keeps_pending_draft() {}\n", ""
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
            self.assertIn("missing required regression test", result.stderr)


if __name__ == "__main__":
    unittest.main()

import subprocess
import tempfile
import textwrap
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPT = ROOT / "scripts" / "verify_ui_thread_contract.py"


def write_ui_lean_fixture(root: Path) -> None:
    generated = root / "generated" / "rust"
    generated.mkdir(parents=True)
    (generated / "AUTOMATION_CERTIFICATE.txt").write_text(
        textwrap.dedent(
            """
            # certui automation certificate
            status: proved
            no_blocking_work_on_ui_thread: proven
            bounded_ui_thread_work: proven
            """
        ).strip()
        + "\n",
        encoding="utf-8",
    )
    (generated / "EXECUTION_CONTRACT.md").write_text(
        "Generated UI handlers are certified to avoid blocking work on the UI thread.\n",
        encoding="utf-8",
    )
    (generated / "README.md").write_text(
        "generated UI dispatch performs no blocking/CPU/wait/custom uncertified work inline on the UI thread\n",
        encoding="utf-8",
    )


class UiThreadContractTests(unittest.TestCase):
    def test_script_defaults_to_repo_sibling_ui_lean(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-thread-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "rust-chatbot"
            (repo_root / "src" / "app" / "ui").mkdir(parents=True)
            (repo_root / "src" / "app" / "state").mkdir(parents=True)
            (repo_root / "src" / "app" / "ui" / "main_panel.rs").write_text(
                "pub fn render() {}\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "state" / "messaging.rs").write_text(
                "pub fn check_for_response() {}\n",
                encoding="utf-8",
            )
            ui_lean_root = Path(tmpdir) / "ui-lean"
            write_ui_lean_fixture(ui_lean_root)

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--repo-root",
                    str(repo_root),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(str(ui_lean_root.resolve()), result.stdout)

    def test_script_defaults_to_parent_of_parent_ui_lean_after_repo_move(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-thread-contract-") as tmpdir:
            workspace_root = Path(tmpdir)
            repo_root = workspace_root / "chatbot" / "rust-chatbot"
            (repo_root / "src" / "app" / "ui").mkdir(parents=True)
            (repo_root / "src" / "app" / "state").mkdir(parents=True)
            (repo_root / "src" / "app" / "ui" / "main_panel.rs").write_text(
                "pub fn render() {}\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "state" / "messaging.rs").write_text(
                "pub fn check_for_response() {}\n",
                encoding="utf-8",
            )
            ui_lean_root = workspace_root / "ui-lean"
            write_ui_lean_fixture(ui_lean_root)

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--repo-root",
                    str(repo_root),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn(str(ui_lean_root.resolve()), result.stdout)

    def test_script_passes_with_background_allow_markers(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-thread-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            (repo_root / "src" / "app" / "ui").mkdir(parents=True)
            (repo_root / "src" / "app" / "state").mkdir(parents=True)
            (repo_root / "src" / "app" / "ui" / "main_panel.rs").write_text(
                textwrap.dedent(
                    """
                    pub fn render() {
                        println!("ok");
                    }
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "ui" / "layout_log.rs").write_text(
                textwrap.dedent(
                    """
                    // ui-thread-guard: allow-blocking-start
                    fn write_layout_log() {
                        std::fs::write("log.txt", "ok").unwrap();
                    }
                    // ui-thread-guard: allow-blocking-end
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "state" / "messaging.rs").write_text(
                "pub fn check_for_response() {}\n",
                encoding="utf-8",
            )
            ui_lean_root = Path(tmpdir) / "ui-lean"
            write_ui_lean_fixture(ui_lean_root)

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--ui-lean-root",
                    str(ui_lean_root),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("ui-thread-contract: PASS", result.stdout)

    def test_script_rejects_inline_blocking_filesystem_call(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-thread-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            (repo_root / "src" / "app" / "ui").mkdir(parents=True)
            (repo_root / "src" / "app" / "state").mkdir(parents=True)
            (repo_root / "src" / "app" / "ui" / "main_panel.rs").write_text(
                textwrap.dedent(
                    """
                    pub fn render() {
                        std::fs::write("log.txt", "bad").unwrap();
                    }
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "state" / "messaging.rs").write_text(
                "pub fn check_for_response() {}\n",
                encoding="utf-8",
            )
            ui_lean_root = Path(tmpdir) / "ui-lean"
            write_ui_lean_fixture(ui_lean_root)

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--ui-lean-root",
                    str(ui_lean_root),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("inline filesystem IO", result.stderr)

    def test_script_rejects_sync_height_cache_access_in_persistence(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-thread-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            (repo_root / "src" / "app" / "ui").mkdir(parents=True)
            (repo_root / "src" / "app" / "state").mkdir(parents=True)
            (repo_root / "src" / "app" / "ui" / "main_panel.rs").write_text(
                "pub fn render() {}\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "state" / "messaging.rs").write_text(
                "pub fn check_for_response() {}\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "state" / "persistence.rs").write_text(
                textwrap.dedent(
                    """
                    pub fn load_session(data_manager: &crate::data::DataManager) {
                        let _ = data_manager.load_height_cache("session-1");
                    }
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )
            ui_lean_root = Path(tmpdir) / "ui-lean"
            write_ui_lean_fixture(ui_lean_root)

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--ui-lean-root",
                    str(ui_lean_root),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("session height-cache access", result.stderr)

    def test_script_rejects_missing_ui_lean_proof_artifact(self) -> None:
        with tempfile.TemporaryDirectory(prefix="ui-thread-contract-") as tmpdir:
            repo_root = Path(tmpdir) / "repo"
            (repo_root / "src" / "app" / "ui").mkdir(parents=True)
            (repo_root / "src" / "app" / "state").mkdir(parents=True)
            (repo_root / "src" / "app" / "ui" / "main_panel.rs").write_text(
                "pub fn render() {}\n",
                encoding="utf-8",
            )
            (repo_root / "src" / "app" / "state" / "messaging.rs").write_text(
                "pub fn check_for_response() {}\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    "python3",
                    str(SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--ui-lean-root",
                    str(Path(tmpdir) / "missing-ui-lean"),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing ui-lean artifact", result.stderr)


if __name__ == "__main__":
    unittest.main()

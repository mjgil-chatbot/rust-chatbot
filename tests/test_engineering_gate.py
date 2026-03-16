import subprocess
import tempfile
import unittest
from pathlib import Path
import textwrap


ROOT = Path(__file__).resolve().parent.parent
HOOK = ROOT / ".githooks" / "pre-commit"
SCRIPT = ROOT / "scripts" / "verify-engineering-gate.sh"
PY_SCRIPT = ROOT / "scripts" / "verify_engineering_gate.py"
REGISTRY = ROOT / "contracts" / "engineering-gate.toml"
UI_THREAD_SCRIPT = ROOT / "scripts" / "verify_ui_thread_contract.py"
UI_SPEC_SCRIPT = ROOT / "scripts" / "verify_ui_spec_contract.py"
UI_SPEC_REGRESSION_SCRIPT = ROOT / "scripts" / "verify-ui-spec-regressions.sh"
SESSION_LIFECYCLE_SCRIPT = ROOT / "scripts" / "verify_session_lifecycle_contract.py"
SESSION_LIFECYCLE_REGRESSION_SCRIPT = (
    ROOT / "scripts" / "verify-session-lifecycle-regressions.sh"
)
SPOOL_SESSION_SCRIPT = ROOT / "scripts" / "verify_spool_session_contract.py"
SPOOL_SESSION_REGRESSION_SCRIPT = (
    ROOT / "scripts" / "verify-spool-session-regressions.sh"
)
HISTORY_CONTRACT_SCRIPT = ROOT / "scripts" / "verify_history_contract.py"
HISTORY_REGRESSION_SCRIPT = ROOT / "scripts" / "verify-history-regressions.sh"
MEMORY_CONTRACT_SCRIPT = ROOT / "scripts" / "verify_memory_contract.py"
MEMORY_REGRESSION_SCRIPT = ROOT / "scripts" / "verify-memory-regressions.sh"
WORKFLOW = ROOT / ".github" / "workflows" / "engineering-gate.yml"


class EngineeringGateTests(unittest.TestCase):
    def test_shell_scripts_parse(self) -> None:
        result = subprocess.run(
            ["bash", "-n", str(HOOK), str(SCRIPT)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_registry_declares_core_gate_entries(self) -> None:
        content = REGISTRY.read_text()
        self.assertIn('repo_id = "rust-chatbot"', content)
        self.assertIn('id = "engineering_docs"', content)
        self.assertIn('id = "session_worker_lifecycle"', content)
        self.assertIn('id = "spool_session_contract"', content)
        self.assertIn('id = "background_flow_runtime"', content)
        self.assertIn('id = "prompt_debug_plaintext_contract"', content)
        self.assertIn('id = "sibling_repo_boundary"', content)
        self.assertIn('id = "ui_thread_contract"', content)
        self.assertIn('id = "ui_send_draft_stream_contract"', content)
        self.assertIn("--repo chatbot-worker --repo __DESKTOP_REPO__", content)
        self.assertIn("scripts/verify-engineering-gate.sh", content)
        self.assertIn("scripts/verify_ui_thread_contract.py", content)
        self.assertIn("scripts/verify_ui_spec_contract.py", content)
        self.assertIn("scripts/verify-ui-spec-regressions.sh", content)
        self.assertIn("scripts/verify_session_lifecycle_contract.py", content)
        self.assertIn("scripts/verify-session-lifecycle-regressions.sh", content)
        self.assertIn("scripts/verify_spool_session_contract.py", content)
        self.assertIn("scripts/verify-spool-session-regressions.sh", content)
        self.assertIn("scripts/verify_history_contract.py", content)
        self.assertIn("scripts/verify-history-regressions.sh", content)
        self.assertIn("scripts/verify_memory_contract.py", content)
        self.assertIn("scripts/verify-memory-regressions.sh", content)
        self.assertIn("tests.test_session_lifecycle_contract", content)
        self.assertIn("tests.test_spool_session_contract", content)
        self.assertIn("tests.test_history_contract", content)
        self.assertIn("tests.test_memory_contract", content)

    def test_hook_runs_engineering_gate_script(self) -> None:
        content = HOOK.read_text()
        self.assertIn('echo "pre-commit: running ui-thread contract verifier"', content)
        self.assertIn('python3 "$repo_root/scripts/verify_ui_thread_contract.py"', content)
        self.assertIn('echo "pre-commit: running ui-spec contract verifier"', content)
        self.assertIn('python3 "$repo_root/scripts/verify_ui_spec_contract.py"', content)
        self.assertIn('echo "pre-commit: running session lifecycle contract verifier"', content)
        self.assertIn('python3 "$repo_root/scripts/verify_session_lifecycle_contract.py"', content)
        self.assertIn('echo "pre-commit: running spool/session contract verifier"', content)
        self.assertIn('python3 "$repo_root/scripts/verify_spool_session_contract.py"', content)
        self.assertIn('echo "pre-commit: running history/checkpoint contract verifier"', content)
        self.assertIn('python3 "$repo_root/scripts/verify_history_contract.py"', content)
        self.assertIn('echo "pre-commit: running memory contract verifier"', content)
        self.assertIn('python3 "$repo_root/scripts/verify_memory_contract.py"', content)
        self.assertIn('echo "pre-commit: running engineering gate"', content)
        self.assertIn('"$repo_root/scripts/verify-engineering-gate.sh"', content)

    def test_ui_thread_script_exists(self) -> None:
        self.assertTrue(UI_THREAD_SCRIPT.is_file())

    def test_ui_spec_scripts_exist(self) -> None:
        self.assertTrue(UI_SPEC_SCRIPT.is_file())
        self.assertTrue(UI_SPEC_REGRESSION_SCRIPT.is_file())

    def test_session_lifecycle_scripts_exist(self) -> None:
        self.assertTrue(SESSION_LIFECYCLE_SCRIPT.is_file())
        self.assertTrue(SESSION_LIFECYCLE_REGRESSION_SCRIPT.is_file())

    def test_spool_session_scripts_exist(self) -> None:
        self.assertTrue(SPOOL_SESSION_SCRIPT.is_file())
        self.assertTrue(SPOOL_SESSION_REGRESSION_SCRIPT.is_file())

    def test_history_scripts_exist(self) -> None:
        self.assertTrue(HISTORY_CONTRACT_SCRIPT.is_file())
        self.assertTrue(HISTORY_REGRESSION_SCRIPT.is_file())

    def test_memory_scripts_exist(self) -> None:
        self.assertTrue(MEMORY_CONTRACT_SCRIPT.is_file())
        self.assertTrue(MEMORY_REGRESSION_SCRIPT.is_file())

    def test_ci_workflow_runs_engineering_gate_with_diff_base(self) -> None:
        content = WORKFLOW.read_text()
        self.assertIn("name: Engineering Gate", content)
        self.assertIn("fetch-depth: 0", content)
        self.assertIn('if [[ ! -d "../${repo}/.git" ]]', content)
        self.assertIn('github.com/mjgil/${repo}.git', content)
        self.assertIn('chatbot-worker', content)
        self.assertIn('chatbot-ui', content)
        self.assertIn('chatbot-history', content)
        self.assertIn('chatbot-adapter', content)
        self.assertIn('ai-providers', content)
        self.assertIn('the-markdown-commons', content)
        self.assertIn('agent-memory', content)
        self.assertIn("scripts/verify-engineering-gate.sh", content)
        self.assertIn("--diff-base", content)
        self.assertIn("ENGINEERING_GATE_TARGET_SUFFIX: ci", content)

    def test_gate_fails_when_protected_code_changes_without_required_docs(self) -> None:
        result = subprocess.run(
            [
                "python3",
                str(PY_SCRIPT),
                "--repo-root",
                str(ROOT),
                "--files",
                "src/app/state/session_runtime.rs",
                "--dry-run",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("session_worker_lifecycle", result.stderr)
        self.assertIn("required documentation", result.stderr)

    def test_gate_passes_when_protected_code_and_required_doc_change_together(self) -> None:
        result = subprocess.run(
            [
                "python3",
                str(PY_SCRIPT),
                "--repo-root",
                str(ROOT),
                "--files",
                "src/app/state/session_runtime.rs",
                "docs/architecture/session-worker-lifecycle-spec.md",
                "docs/verification.md",
                "--dry-run",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("session_worker_lifecycle", result.stdout)
        self.assertIn("tests.test_engineering_assessment_docs", result.stdout)

    def test_gate_passes_for_docs_contract_in_dry_run_mode(self) -> None:
        result = subprocess.run(
            [
                "python3",
                str(PY_SCRIPT),
                "--repo-root",
                str(ROOT),
                "--files",
                "docs/architecture/evidence-gate.md",
                "--dry-run",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("engineering_docs", result.stdout)
        self.assertIn("tests.test_engineering_gate", result.stdout)

    def test_gate_can_run_named_command_group(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rust-chatbot-group-") as tmpdir:
            repo_root = Path(tmpdir) / "rust-chatbot"
            (repo_root / "contracts").mkdir(parents=True)
            (repo_root / "contracts" / "engineering-gate.toml").write_text(
                textwrap.dedent(
                    """
                    version = 1
                    repo_id = "rust-chatbot"
                    repo_root_names = ["rust-chatbot"]

                    [[command_group]]
                    id = "demo_group"
                    commands = ['python3 -c "print(\\"ok\\")"']
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            result = subprocess.run(
                [
                    "python3",
                    str(PY_SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--registry",
                    str(repo_root / "contracts" / "engineering-gate.toml"),
                    "--run-group",
                    "demo_group",
                    "--dry-run",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("active command groups", result.stdout)
            self.assertIn("demo_group", result.stdout)
            self.assertIn('python3 -c "print(\\"ok\\")"', result.stdout)

    def test_gate_uses_real_staged_file_discovery(self) -> None:
        with tempfile.TemporaryDirectory(prefix="repo-under-test-") as tmpdir:
            repo_root = Path(tmpdir) / "repo-under-test"
            (repo_root / "contracts").mkdir(parents=True)
            (repo_root / "src").mkdir()
            (repo_root / "docs").mkdir()
            (repo_root / "src" / "main.rs").write_text("fn main() {}\n")
            (repo_root / "docs" / "spec.md").write_text("# spec\n")
            (repo_root / "contracts" / "engineering-gate.toml").write_text(
                textwrap.dedent(
                    """
                    version = 1
                    repo_id = "repo-under-test"
                    repo_root_names = ["repo-under-test"]

                    [[contract]]
                    id = "staged_contract"
                    description = "staged file discovery coverage"
                    trigger_globs = ["src/*.rs", "src/**/*.rs"]
                    required_any_staged = ["docs/spec.md"]
                    commands = ['python3 -c "print(\"ok\")"']
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            subprocess.run(["git", "init"], cwd=repo_root, check=True, capture_output=True, text=True)
            subprocess.run(
                ["git", "add", "src/main.rs", "docs/spec.md", "contracts/engineering-gate.toml"],
                cwd=repo_root,
                check=True,
                capture_output=True,
                text=True,
            )

            result = subprocess.run(
                [
                    "python3",
                    str(PY_SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--registry",
                    str(repo_root / "contracts" / "engineering-gate.toml"),
                    "--dry-run",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("engineering-gate[repo-under-test]: active contracts", result.stdout)
            self.assertIn("staged_contract", result.stdout)

    def test_gate_uses_diff_range_file_discovery(self) -> None:
        with tempfile.TemporaryDirectory(prefix="rust-chatbot-diff-") as tmpdir:
            repo_root = Path(tmpdir) / "rust-chatbot"
            (repo_root / "contracts").mkdir(parents=True)
            (repo_root / "src" / "app" / "state").mkdir(parents=True)
            (repo_root / "docs").mkdir()
            (repo_root / "src" / "app" / "state" / "session_runtime.rs").write_text(
                "pub fn demo() {}\n"
            )
            (repo_root / "docs" / "verification.md").write_text("# verification\n")
            (repo_root / "contracts" / "engineering-gate.toml").write_text(
                textwrap.dedent(
                    """
                    version = 1
                    repo_id = "rust-chatbot"
                    repo_root_names = ["rust-chatbot"]

                    [[contract]]
                    id = "diff_contract"
                    description = "diff discovery coverage"
                    trigger_globs = ["src/app/state/*.rs", "src/app/state/**/*.rs"]
                    required_any_staged = ["docs/verification.md"]
                    commands = ['python3 -c "print(\\"ok\\")"']
                    """
                ).strip()
                + "\n",
                encoding="utf-8",
            )

            subprocess.run(["git", "init"], cwd=repo_root, check=True, capture_output=True, text=True)
            subprocess.run(
                ["git", "config", "user.name", "Rust Chatbot Test"],
                cwd=repo_root,
                check=True,
                capture_output=True,
                text=True,
            )
            subprocess.run(
                ["git", "config", "user.email", "rust-chatbot-test@example.com"],
                cwd=repo_root,
                check=True,
                capture_output=True,
                text=True,
            )
            subprocess.run(["git", "add", "."], cwd=repo_root, check=True, capture_output=True, text=True)
            subprocess.run(
                ["git", "commit", "-m", "base"],
                cwd=repo_root,
                check=True,
                capture_output=True,
                text=True,
            )

            (repo_root / "src" / "app" / "state" / "session_runtime.rs").write_text(
                "pub fn demo() { println!(\"changed\"); }\n"
            )
            (repo_root / "docs" / "verification.md").write_text("# verification\nchanged\n")
            subprocess.run(
                ["git", "add", "src/app/state/session_runtime.rs", "docs/verification.md"],
                cwd=repo_root,
                check=True,
                capture_output=True,
                text=True,
            )
            subprocess.run(
                ["git", "commit", "-m", "head"],
                cwd=repo_root,
                check=True,
                capture_output=True,
                text=True,
            )

            result = subprocess.run(
                [
                    "python3",
                    str(PY_SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--registry",
                    str(repo_root / "contracts" / "engineering-gate.toml"),
                    "--diff-base",
                    "HEAD~1",
                    "--diff-head",
                    "HEAD",
                    "--dry-run",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("engineering-gate[rust-chatbot]: active contracts", result.stdout)
            self.assertIn("diff_contract", result.stdout)

    def test_gate_rejects_registry_for_different_repo_root(self) -> None:
        with tempfile.TemporaryDirectory(prefix="other-repo-") as tmpdir:
            repo_root = Path(tmpdir) / "other-repo"
            repo_root.mkdir()
            result = subprocess.run(
                [
                    "python3",
                    str(PY_SCRIPT),
                    "--repo-root",
                    str(repo_root),
                    "--registry",
                    str(REGISTRY),
                    "--dry-run",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("registry for rust-chatbot cannot run from repo root 'other-repo'", result.stderr)


if __name__ == "__main__":
    unittest.main()

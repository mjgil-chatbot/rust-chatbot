import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
SCRIPTS = ROOT / "scripts"
FULL_SCRIPT = ROOT / "scripts" / "auto-ui-test-full.sh"
PROMPT_WRAPPER = ROOT / "scripts" / "auto-ui-test-prompt-debug.sh"
BACKGROUND_FLOW_WRAPPER = ROOT / "scripts" / "auto-ui-test-background-flow.sh"
PROMPT_DEBUG = ROOT / "scripts" / "auto_ui_prompt_debug.sh"
DEBUG_SUPPORT = ROOT / "scripts" / "auto_ui_debug_support.py"
SEMBLE_WRAPPER = ROOT / "scripts" / "bin" / "semble"
COMMON_SH = ROOT / "scripts" / "common.sh"

sys.path.insert(0, str(SCRIPTS))

import auto_ui_debug_support as debug_support
import auto_ui_header_debug as header_debug


class AutoUiShellScriptTests(unittest.TestCase):
    def touch(self, path: Path, contents: str = "") -> None:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents)

    def test_shell_scripts_parse(self) -> None:
        result = subprocess.run(
            [
                "bash",
                "-n",
                str(FULL_SCRIPT),
                str(BACKGROUND_FLOW_WRAPPER),
                str(PROMPT_WRAPPER),
                str(PROMPT_DEBUG),
                str(SEMBLE_WRAPPER),
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_full_suite_mentions_prompt_debug_wrapper_contract(self) -> None:
        content = FULL_SCRIPT.read_text()
        self.assertIn("USE_REAL_DISPLAY=1", content)
        self.assertIn("FIXTURE_PROMPT_DEBUG_PROVIDER", content)
        self.assertIn("FIXTURE_PROMPT_DEBUG_SESSION_ID", content)
        self.assertIn("FIXTURE_PROMPT_DEBUG_PROMPT", content)
        self.assertIn("FIXTURE_PROMPT_DEBUG_REQUIRE_SUBSTRING", content)
        self.assertIn("SKIP_FAST", content)
        self.assertIn("PROMPT_DEBUG_SESSION_ID", content)
        self.assertIn("PROMPT_DEBUG_SESSION_JSON", content)
        self.assertIn("PROMPT_DEBUG_TRACE_TIMEOUT", content)
        self.assertIn("PROMPT_DEBUG_REQUIRE_TOOL_TRANSCRIPT_COUNT", content)
        self.assertIn("PROMPT_DEBUG_FORBID_TOOL_TRANSCRIPT_SUBSTRING", content)
        self.assertIn("auto-ui-test-prompt-debug.sh", content)
        self.assertIn('HEADLESS_FLAG="--headless"', content)
        self.assertIn('"$AUTO_UI_BIN" run $HEADLESS_FLAG --config "$PROMPT_TOML"', content)

    def test_background_flow_wrapper_uses_isolated_target_dir(self) -> None:
        content = BACKGROUND_FLOW_WRAPPER.read_text()
        self.assertIn('TARGET_DIR="${CARGO_TARGET_DIR:-${APP_ROOT}/target/auto-ui-background-flow}"', content)
        self.assertIn('export CARGO_TARGET_DIR="${TARGET_DIR}"', content)

    def test_common_sh_installs_managed_semble_wrapper(self) -> None:
        content = COMMON_SH.read_text()
        self.assertIn(
            'rust_chatbot_install_managed_wrapper "$repo_root/scripts/bin/semble" "$bin_dir/semble"',
            content,
        )

    def test_semble_wrapper_prefers_release_binary_and_falls_back_to_cargo(self) -> None:
        content = SEMBLE_WRAPPER.read_text()
        self.assertIn('SEMBLE_ROOT="${RUST_CHATBOT_SEMBLE_RS_ROOT:-$HOME/git/semble-rs/semble-rs-work}"', content)
        self.assertIn('SEMBLE_BIN="$SEMBLE_ROOT/target/release/semble"', content)
        self.assertIn('if [ -e "$path" ] && find "$path" -type f -newer "$SEMBLE_BIN" -print -quit 2>/dev/null | grep -q .; then', content)
        self.assertIn('exec "$SEMBLE_BIN" "$@"', content)
        self.assertIn(
            'exec cargo run --release --manifest-path "$SEMBLE_MANIFEST" --bin semble -- "$@"',
            content,
        )

    def test_full_suite_seeds_gemini_oauth_for_worker_and_forge(self) -> None:
        content = FULL_SCRIPT.read_text()
        self.assertIn('mkdir -p "$TMP_HOME/.rust-chatbot/.gemini"', content)
        self.assertIn(
            '"$TMP_HOME/.rust-chatbot/.gemini/oauth_creds.json"',
            content,
        )
        self.assertIn('mkdir -p "$TMP_HOME/.gemini"', content)
        self.assertIn(
            '"$TMP_HOME/.gemini/oauth_creds.json"',
            content,
        )

    def test_full_suite_seeds_additional_gemini_home_state(self) -> None:
        content = FULL_SCRIPT.read_text()
        self.assertIn(
            '"$REAL_HOME/.rust-chatbot/.gemini/api.key"',
            content,
        )
        self.assertIn(
            '"$TMP_HOME/.rust-chatbot/.gemini/api.key"',
            content,
        )
        self.assertIn(
            '"$TMP_HOME/.gemini/api.key"',
            content,
        )
        self.assertIn("for gemini_state_file in \\", content)
        for filename in (
            "settings.json",
            "google_accounts.json",
            "projects.json",
            "installation_id",
        ):
            self.assertIn(f"  {filename} \\", content)
        self.assertIn("  trustedFolders.json; do", content)
        self.assertIn(
            '"$REAL_HOME/.gemini/$gemini_state_file"',
            content,
        )
        self.assertIn(
            '"$TMP_HOME/.gemini/$gemini_state_file"',
            content,
        )

    def test_full_suite_seeds_gcloud_state_for_gemini_forge_bearer_auth(self) -> None:
        content = FULL_SCRIPT.read_text()
        self.assertIn(
            '"$REAL_HOME/.config/gcloud/application_default_credentials.json"',
            content,
        )
        self.assertIn(
            '"$TMP_HOME/.config/gcloud/application_default_credentials.json"',
            content,
        )
        self.assertIn(
            "for gcloud_state_file in \\",
            content,
        )
        self.assertIn(
            "  active_config \\",
            content,
        )
        self.assertIn(
            "  access_tokens.db \\",
            content,
        )
        self.assertIn(
            "  config_sentinel \\",
            content,
        )
        self.assertIn(
            "  credentials.db \\",
            content,
        )
        self.assertIn(
            "  default_configs.db; do",
            content,
        )
        self.assertIn(
            '"$REAL_HOME/.config/gcloud/$gcloud_state_file"',
            content,
        )
        self.assertIn(
            '"$TMP_HOME/.config/gcloud/$gcloud_state_file"',
            content,
        )
        self.assertIn(
            '"$REAL_HOME/.config/gcloud/configurations/config_default"',
            content,
        )
        self.assertIn(
            '"$TMP_HOME/.config/gcloud/configurations/config_default"',
            content,
        )
        self.assertIn(
            '"$REAL_HOME/.config/gcloud/legacy_credentials"',
            content,
        )
        self.assertIn(
            '"$TMP_HOME/.config/gcloud/legacy_credentials"',
            content,
        )

    def test_auto_ui_launch_skips_backup_scheduler_preflight(self) -> None:
        content = DEBUG_SUPPORT.read_text()
        self.assertIn(
            'env["RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT"] = "1"',
            content,
        )

    def test_auto_ui_release_stack_is_stale_when_build_inputs_are_newer(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            app_root = temp_root / "rust-chatbot"
            adapter_root = temp_root / "chatbot-adapter"

            self.touch(app_root / "Cargo.toml", "[package]\nname='rust-chatbot'\n")
            self.touch(app_root / "src" / "main.rs", "fn main() {}\n")
            self.touch(adapter_root / "Cargo.toml", "[workspace]\n")
            self.touch(
                adapter_root / "crates" / "agent-adapter-cli" / "src" / "main.rs",
                "fn main() {}\n",
            )

            outputs = [
                app_root / "target" / "release" / "chatbot-ctl",
                app_root / "target" / "release" / "rust-chatbot",
                app_root / "target" / "release" / "rcc",
                app_root / "target" / "release" / "chatbot-worker",
                adapter_root / "target" / "release" / "agent-adapter-cli",
            ]
            for output in outputs:
                self.touch(output, "binary\n")

            stale_time = 1_000
            fresh_time = 2_000
            for output in outputs:
                output.touch()
                os.utime(output, (stale_time, stale_time))
            os.utime(app_root / "Cargo.toml", (stale_time, stale_time))
            os.utime(adapter_root / "Cargo.toml", (stale_time, stale_time))
            os.utime(app_root / "src" / "main.rs", (fresh_time, fresh_time))
            os.utime(
                adapter_root / "crates" / "agent-adapter-cli" / "src" / "main.rs",
                (stale_time, stale_time),
            )

            old_adapter_root = os.environ.get("RUST_CHATBOT_ADAPTER_ROOT")
            os.environ["RUST_CHATBOT_ADAPTER_ROOT"] = str(adapter_root)
            try:
                self.assertTrue(debug_support.release_stack_is_stale(app_root))
            finally:
                if old_adapter_root is None:
                    os.environ.pop("RUST_CHATBOT_ADAPTER_ROOT", None)
                else:
                    os.environ["RUST_CHATBOT_ADAPTER_ROOT"] = old_adapter_root

    def test_auto_ui_release_stack_is_fresh_when_outputs_cover_build_inputs(self) -> None:
        with tempfile.TemporaryDirectory() as temp_dir:
            temp_root = Path(temp_dir)
            app_root = temp_root / "rust-chatbot"
            adapter_root = temp_root / "chatbot-adapter"

            self.touch(app_root / "Cargo.toml", "[package]\nname='rust-chatbot'\n")
            self.touch(app_root / "src" / "main.rs", "fn main() {}\n")
            self.touch(adapter_root / "Cargo.toml", "[workspace]\n")
            self.touch(
                adapter_root / "crates" / "agent-adapter-cli" / "src" / "main.rs",
                "fn main() {}\n",
            )

            outputs = [
                app_root / "target" / "release" / "chatbot-ctl",
                app_root / "target" / "release" / "rust-chatbot",
                app_root / "target" / "release" / "rcc",
                app_root / "target" / "release" / "chatbot-worker",
                adapter_root / "target" / "release" / "agent-adapter-cli",
            ]
            for output in outputs:
                self.touch(output, "binary\n")

            source_time = 1_000
            fresh_time = 2_000
            os.utime(app_root / "Cargo.toml", (source_time, source_time))
            os.utime(app_root / "src" / "main.rs", (source_time, source_time))
            os.utime(adapter_root / "Cargo.toml", (source_time, source_time))
            os.utime(
                adapter_root / "crates" / "agent-adapter-cli" / "src" / "main.rs",
                (source_time, source_time),
            )
            for output in outputs:
                output.touch()
                os.utime(output, (fresh_time, fresh_time))

            old_adapter_root = os.environ.get("RUST_CHATBOT_ADAPTER_ROOT")
            os.environ["RUST_CHATBOT_ADAPTER_ROOT"] = str(adapter_root)
            try:
                self.assertFalse(debug_support.release_stack_is_stale(app_root))
            finally:
                if old_adapter_root is None:
                    os.environ.pop("RUST_CHATBOT_ADAPTER_ROOT", None)
                else:
                    os.environ["RUST_CHATBOT_ADAPTER_ROOT"] = old_adapter_root

    def test_omniforge_auto_ui_support_maps_to_dedicated_title_and_data_dir(self) -> None:
        self.assertEqual(
            debug_support.provider_title("omniforge", None),
            "OmniForge Rust Chatbot ~ /rust-chatbot",
        )
        self.assertEqual(
            debug_support.provider_data_dir("omniforge").name,
            ".omniforge-desktop",
        )

    def test_gemini_cli_forge_auto_ui_support_maps_to_dedicated_title_and_data_dir(self) -> None:
        self.assertEqual(
            debug_support.provider_title("geminicliforge", None),
            "GeminiCliForge Rust Chatbot ~ /rust-chatbot",
        )
        self.assertEqual(
            debug_support.provider_data_dir("geminicliforge").name,
            ".gemini-cli-forge-desktop",
        )

    def test_minimax_auto_ui_support_maps_to_dedicated_title_and_data_dir(self) -> None:
        self.assertEqual(
            debug_support.provider_title("minimax", None),
            "MiniMax Rust Chatbot ~ /rust-chatbot",
        )
        self.assertEqual(
            debug_support.provider_data_dir("minimax").name,
            ".minimax-desktop",
        )

    def test_omniforge_header_debug_uses_dedicated_session_field(self) -> None:
        self.assertEqual(
            header_debug.provider_session_field("omniforge"),
            "omniforge_session_id",
        )

    def test_gemini_cli_forge_header_debug_uses_gemini_session_field(self) -> None:
        self.assertEqual(
            header_debug.provider_session_field("geminicliforge"),
            "gemini_session_id",
        )

    def test_minimax_header_debug_uses_minimax_session_field(self) -> None:
        self.assertEqual(
            header_debug.provider_session_field("minimax"),
            "minimax_session_id",
        )

    def test_full_suite_supports_minimax_fixture_prompt_debug(self) -> None:
        content = FULL_SCRIPT.read_text()
        self.assertIn('minimax) fixture_provider_dir=".minimax-desktop" ;;', content)


if __name__ == "__main__":
    unittest.main()

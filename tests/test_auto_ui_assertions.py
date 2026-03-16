import importlib.util
import json
import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT_PATH = Path(__file__).resolve().parent.parent / "scripts" / "auto-ui-assertions.py"
WRAPPER_PATH = Path(__file__).resolve().parent.parent / "scripts" / "auto-ui-test-prompt-debug.sh"
SPEC = importlib.util.spec_from_file_location("auto_ui_assertions", SCRIPT_PATH)
MODULE = importlib.util.module_from_spec(SPEC)
assert SPEC is not None and SPEC.loader is not None
SPEC.loader.exec_module(MODULE)


class PromptDebugAssertionsTests(unittest.TestCase):
    def make_prompt_debug_report(self, screenshot_path: Path) -> dict:
        return {
            "status": "ok",
            "scenario": "prompt_debug",
            "artifacts": [
                {
                    "kind": "window_screenshot",
                    "path": str(screenshot_path),
                }
            ],
            "measurements": [
                {
                    "ai_response_end": {"status": "done"},
                    "last_markdown_row": None,
                }
            ],
            "details": {},
        }

    def make_session_json(self, path: Path, content: str) -> None:
        path.write_text(
            json.dumps(
                {
                    "messages": [
                        {"is_user": True, "content": ":muse ..."},
                        {"is_user": False, "content": content},
                    ]
                }
            )
        )

    def test_prompt_debug_plaintext_stream_accepts_normalized_ascii_content(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "+-------+--------+\n| Phase | Focus  |\n+-------+--------+\nfn main() {}",
            )

            MODULE.assert_prompt_debug_plaintext_stream(
                self.make_prompt_debug_report(screenshot),
                session_json,
                ["+-------+--------+", "fn main() {}"],
            )

    def test_prompt_debug_plaintext_stream_rejects_box_drawing_glyphs(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            session_json = tmp / "session.json"
            self.make_session_json(session_json, "┌───────┬────────┐\n│ Phase │ Focus  │")

            with self.assertRaises(MODULE.AssertionFailure):
                MODULE.assert_prompt_debug_plaintext_stream(
                    self.make_prompt_debug_report(screenshot),
                    session_json,
                    [],
                )

    def test_prompt_debug_plaintext_stream_accepts_required_tool_transcript_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "● [00:00:01] Execute [/bin/bash] pwd\n"
                "● [00:00:02] Execute [/bin/bash] git status --short\n"
                "Summary",
            )

            MODULE.assert_prompt_debug_plaintext_stream(
                self.make_prompt_debug_report(screenshot),
                session_json,
                ["Summary"],
                2,
                ["pwd", "git status --short"],
            )

    def test_prompt_debug_plaintext_stream_accepts_timestamped_transcript_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "[May 20 11:34pm] I’ll inspect the adapter path first.\n"
                "[May 20 11:34pm] MM-2.7 | Semble `search \"CLI argument parsing\" /tmp/project`\n"
                "[May 20 11:34pm] MM-2.7 | FFF `parse_args_from src`\n"
                "Summary",
            )

            MODULE.assert_prompt_debug_plaintext_stream(
                self.make_prompt_debug_report(screenshot),
                session_json,
                ["Summary"],
                2,
                ["Semble", "FFF"],
            )

    def test_prompt_debug_plaintext_stream_rejects_missing_tool_transcript_lines(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            session_json = tmp / "session.json"
            self.make_session_json(session_json, "1. `pwd`\n2. `git status --short`\n")

            with self.assertRaises(MODULE.AssertionFailure):
                MODULE.assert_prompt_debug_plaintext_stream(
                    self.make_prompt_debug_report(screenshot),
                    session_json,
                    [],
                    2,
                    ["pwd"],
                )

    def test_prompt_debug_plaintext_stream_rejects_generic_tool_label(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "[May 20 11:34pm] MM-2.7 | tool | Search results for: 'CLI argument parsing'\n"
                "Summary",
            )

            with self.assertRaises(MODULE.AssertionFailure):
                MODULE.assert_prompt_debug_plaintext_stream(
                    self.make_prompt_debug_report(screenshot),
                    session_json,
                    ["Summary"],
                )

    def test_prompt_debug_plaintext_stream_rejects_forbidden_tool_transcript_substring(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "● [10:59:01pm] MM-2.7 | Shell\n"
                "● [10:59:02pm] MM-2.7 | Read File\n"
                "Summary",
            )

            with self.assertRaises(MODULE.AssertionFailure):
                MODULE.assert_prompt_debug_plaintext_stream(
                    self.make_prompt_debug_report(screenshot),
                    session_json,
                    ["Summary"],
                    2,
                    [],
                    ["| Shell"],
                )


class PromptDebugWrapperTests(unittest.TestCase):
    def make_prompt_debug_report(self, screenshot_path: Path) -> dict:
        return {
            "status": "ok",
            "scenario": "prompt_debug",
            "artifacts": [
                {
                    "kind": "window_screenshot",
                    "path": str(screenshot_path),
                }
            ],
            "measurements": [
                {
                    "ai_response_end": {"status": "done"},
                    "last_markdown_row": None,
                }
            ],
            "details": {},
        }

    def make_session_json(self, path: Path, content: str) -> None:
        path.write_text(
            json.dumps(
                {
                    "messages": [
                        {"is_user": True, "content": ":muse ..."},
                        {"is_user": False, "content": content},
                    ]
                }
            )
        )

    def test_assert_only_wrapper_passes_with_valid_report_and_session(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            report = tmp / "report.json"
            report.write_text(json.dumps(self.make_prompt_debug_report(screenshot)))
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "● [00:00:01] Execute [/bin/bash] pwd\n"
                "● [00:00:02] Execute [/bin/bash] git status --short\n"
                "+-----+\nfn main() {}",
            )

            result = subprocess.run(
                [
                    "bash",
                    str(WRAPPER_PATH),
                    "--assert-only",
                    "--output-dir",
                    str(tmp),
                    "--session-json",
                    str(session_json),
                    "--require-substring",
                    "fn main() {}",
                    "--require-tool-transcript-count",
                    "2",
                    "--require-tool-transcript-substring",
                    "pwd",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("prompt_debug assertions passed", result.stdout)

    def test_assert_only_wrapper_fails_without_report_json(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            session_json = tmp / "session.json"
            self.make_session_json(session_json, "fn main() {}")

            result = subprocess.run(
                [
                    "bash",
                    str(WRAPPER_PATH),
                    "--assert-only",
                    "--output-dir",
                    str(tmp),
                    "--session-json",
                    str(session_json),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Missing report.json", result.stderr)


class DebugScenarioAssertionsTests(unittest.TestCase):
    def make_prompt_debug_report(self, screenshot_path: Path) -> dict:
        return {
            "status": "ok",
            "scenario": "prompt_debug",
            "artifacts": [
                {
                    "kind": "window_screenshot",
                    "path": str(screenshot_path),
                }
            ],
            "measurements": [
                {
                    "ai_response_end": {"status": "done"},
                    "last_markdown_row": None,
                }
            ],
            "details": {},
        }

    def make_session_json(self, path: Path, content: str) -> None:
        path.write_text(
            json.dumps(
                {
                    "messages": [
                        {"is_user": True, "content": ":muse ..."},
                        {"is_user": False, "content": content},
                    ]
                }
            )
        )

    def make_debug_report(self, trace_log: Path) -> dict:
        return {
            "status": "ok",
            "scenario": "debug",
            "details": {
                "widths": [900],
                "trace_log": str(trace_log),
                "new_chat_ordering": {
                    "requested_name": "Created",
                    "trace": {
                        "session_id": "created-id",
                        "session_name": "Created",
                        "sidebar_top_session_id": "created-id",
                        "sidebar_top_session_name": "Created",
                        "sidebar_selected_session_id": "created-id",
                    },
                },
                "sessions": [
                    {
                        "requested_width": 900,
                        "sessions": [
                            {
                                "session_id": "sid-1",
                                "session_name": MODULE.DEFAULT_SESSION_NAMES[0],
                                "text_visible_heuristic": True,
                                "visual_metric": {"stddev": 0.2, "colors": 32.0},
                                "trace": {
                                    "message_count": "2",
                                    "rendered_rows": "2",
                                    "first_rect_height": "120.0",
                                },
                            }
                        ],
                    }
                ],
            },
        }

    def test_new_chat_sidebar_ordering_accepts_created_session_at_top(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            trace_log = tmp / "trace.log"
            trace_log.write_text("", encoding="utf-8")

            MODULE.assert_new_chat_sidebar_ordering(self.make_debug_report(trace_log))

    def test_new_chat_sidebar_ordering_rejects_created_session_not_at_top(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            trace_log = tmp / "trace.log"
            trace_log.write_text("", encoding="utf-8")
            report = self.make_debug_report(trace_log)
            report["details"]["new_chat_ordering"]["trace"]["sidebar_top_session_id"] = "other-id"

            with self.assertRaises(MODULE.AssertionFailure):
                MODULE.assert_new_chat_sidebar_ordering(report)

    def test_session_switch_offload_accepts_pending_then_clear_transition(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            trace_log = tmp / "trace.log"
            trace_log.write_text(
                "\n".join(
                    [
                        'info ui_auto_debug session_id=sid-1 height_cache_load_pending=true height_cache_present=false',
                        'info ui_auto_debug session_id=sid-1 height_cache_load_pending=false height_cache_present=false',
                    ]
                )
                + "\n",
                encoding="utf-8",
            )

            MODULE.assert_session_switch_offload(
                self.make_debug_report(trace_log),
                trace_log,
            )

    def test_session_switch_offload_rejects_missing_pending_transition(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            trace_log = tmp / "trace.log"
            trace_log.write_text(
                'info ui_auto_debug session_id=sid-1 height_cache_load_pending=false height_cache_present=false\n',
                encoding="utf-8",
            )

            with self.assertRaises(MODULE.AssertionFailure):
                MODULE.assert_session_switch_offload(
                    self.make_debug_report(trace_log),
                    trace_log,
                )

    def test_assert_only_wrapper_rejects_forbidden_tool_transcript_substring(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            report = tmp / "report.json"
            report.write_text(json.dumps(self.make_prompt_debug_report(screenshot)))
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "● [10:59:01pm] MM-2.7 | Shell\n"
                "● [10:59:02pm] MM-2.7 | Read File\n"
                "fn main() {}",
            )

            result = subprocess.run(
                [
                    "bash",
                    str(WRAPPER_PATH),
                    "--assert-only",
                    "--output-dir",
                    str(tmp),
                    "--session-json",
                    str(session_json),
                    "--forbid-tool-transcript-substring",
                    "| Shell",
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("forbidden tool substring", result.stderr)

    def test_assert_only_wrapper_rejects_generic_tool_label(self) -> None:
        with tempfile.TemporaryDirectory() as tmpdir:
            tmp = Path(tmpdir)
            screenshot = tmp / "window.png"
            screenshot.write_bytes(b"\x89PNG\r\n\x1a\n" + b"x" * 4096)
            report = tmp / "report.json"
            report.write_text(json.dumps(self.make_prompt_debug_report(screenshot)))
            session_json = tmp / "session.json"
            self.make_session_json(
                session_json,
                "[May 20 11:34pm] MM-2.7 | tool | Search results for: 'CLI argument parsing'\n"
                "fn main() {}",
            )

            result = subprocess.run(
                [
                    "bash",
                    str(WRAPPER_PATH),
                    "--assert-only",
                    "--output-dir",
                    str(tmp),
                    "--session-json",
                    str(session_json),
                ],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("generic `tool` labels", result.stderr)


if __name__ == "__main__":
    unittest.main()

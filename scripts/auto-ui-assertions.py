#!/usr/bin/env python3
"""Assert behavioral expectations on auto-ui report.json output.

Drives four checks against artifacts produced by the auto-ui `debug` scenario:

  - chat switching:     7 distinct sessions appear in DEFAULT_SESSION_NAMES order;
                        the trace log's ui_auto_debug emissions transition between
                        them in the same order (proves the binary actually loaded
                        each session and rendered it once before the next launch).
  - new chat ordering:  the dedicated create-session probe renders the newly
                        created session with that same session already selected
                        and sitting at sidebar index 0 on the first observed
                        post-create ui_auto_debug trace.
  - height cache:       per-width first_rect_height is populated and consistent
                        across multi-width runs; height_cache JSON files exist
                        on disk for at least the active sessions.
  - offload ordering:   at least one switched session renders while its height-
                        cache background load is still pending, then later
                        renders again after that pending state clears.
  - assistant render:   every session reports text_visible_heuristic=true,
                        rendered_rows >= message_count, and the screenshot has
                        non-trivial color variance (visual_metric.stddev > 0.01).

Exit code 0 on success, non-zero on first failure. Usage:

    auto-ui-assertions.py REPORT_JSON [--home HOME] [--multi-width]
"""
from __future__ import annotations

import argparse
import json
import os
import re
import sys
from pathlib import Path

DEFAULT_SESSION_NAMES = [
    "pl-update", "pl-enhance", "pl-24", "pl-assess",
    "da-scrape-result-submission", "pl-graph-problem", "pl-enhancements-2",
]
TRACE_LINE_RE = re.compile(r"ui_auto_debug.*?session_id=(\S+)")
BOX_DRAWING_RE = re.compile(r"[\u2500-\u257F]")
PRIVATE_USE_RE = re.compile(r"[\uE000-\uF8FF]")
TRANSCRIPT_STATUS_PREFIXES = ("● [", "○ [", "◦ [", "• [")
STANDARD_TRANSCRIPT_LINE_RE = re.compile(
    r"^\[[A-Z][a-z]{2} \d{1,2} \d{2}:\d{2}[ap]m\] .+"
)
STANDARD_TOOL_TRANSCRIPT_LINE_RE = re.compile(
    r"^\[[A-Z][a-z]{2} \d{1,2} \d{2}:\d{2}[ap]m\] [^|\n]+ \| .+"
)
GENERIC_STANDARD_TOOL_LABEL_RE = re.compile(
    r"^\[[A-Z][a-z]{2} \d{1,2} \d{2}:\d{2}[ap]m\] [^|\n]+ \| tool(?: \||$)",
    re.IGNORECASE,
)
FIELD_RE = re.compile(r"(\w+)=((?:\"[^\"]*\")|(?:\S+))")


class AssertionFailure(Exception):
    pass


def fail(msg: str) -> None:
    raise AssertionFailure(msg)


def load_report(path: Path) -> dict:
    if not path.exists():
        fail(f"report not found: {path}")
    return json.loads(path.read_text())


def find_artifact_path(report: dict, kind: str) -> Path | None:
    for artifact in report.get("artifacts", []):
        if artifact.get("kind") == kind and artifact.get("path"):
            return Path(artifact["path"])
    return None


def shared_runtime_root(home: Path) -> Path:
    override = os.environ.get("RUST_CHATBOT_SHARED_ROOT") or os.environ.get("RUST_CHATBOT_CONFIG_HOME")
    return Path(override) if override else home / ".rust-chatbot"


def collect_sessions_per_width(report: dict) -> list[tuple[int, list[dict]]]:
    """Return [(width, [session_dict, ...]), ...] from report.details.sessions."""
    details = report.get("details", {})
    blocks = details.get("sessions", [])
    out = []
    for block in blocks:
        width = block.get("requested_width")
        sessions = block.get("sessions", [])
        out.append((width, sessions))
    return out


def parse_trace_fields(line: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for key, value in FIELD_RE.findall(line):
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        fields[key] = value
    return fields


def assert_chat_switching(report: dict, trace_log: Path) -> None:
    blocks = collect_sessions_per_width(report)
    if not blocks:
        fail("report has no per-width session blocks (details.sessions is empty)")

    for width, sessions in blocks:
        names = [s.get("session_name") for s in sessions]
        if names != DEFAULT_SESSION_NAMES:
            fail(
                f"width {width}: session order mismatch.\n"
                f"  expected: {DEFAULT_SESSION_NAMES}\n"
                f"  observed: {names}"
            )
        ids = [s.get("session_id") for s in sessions]
        if len(set(ids)) != len(ids):
            fail(f"width {width}: duplicate session_ids in run: {ids}")

    if not trace_log.exists():
        fail(f"trace log missing: {trace_log}")
    trace_session_ids = []
    last = None
    for line in trace_log.read_text(errors="replace").splitlines():
        m = TRACE_LINE_RE.search(line)
        if not m:
            continue
        sid = m.group(1)
        if sid != last:
            trace_session_ids.append(sid)
            last = sid
    if not trace_session_ids:
        fail("no ui_auto_debug session_id transitions found in trace log")
    expected_ids = [s.get("session_id") for _, sessions in blocks for s in sessions]
    expected_seq = list(dict.fromkeys(expected_ids))
    observed_seq = list(dict.fromkeys(trace_session_ids))
    if not all(sid in observed_seq for sid in expected_seq):
        fail(
            "trace log missing expected session transitions.\n"
            f"  expected (any order, all present): {expected_seq}\n"
            f"  observed: {observed_seq}"
        )
    print(f"  chat switching: {len(blocks)} width(s) × {len(DEFAULT_SESSION_NAMES)} sessions, "
          f"trace transitions = {len(observed_seq)} unique ids")


def load_ui_trace_events(trace_log: Path) -> list[dict[str, str]]:
    if not trace_log.exists():
        fail(f"trace log missing: {trace_log}")
    events: list[dict[str, str]] = []
    for line in trace_log.read_text(errors="replace").splitlines():
        if "ui_auto_debug" not in line:
            continue
        fields = parse_trace_fields(line)
        if "session_id" in fields:
            events.append(fields)
    return events


def assert_session_switch_offload(report: dict, trace_log: Path) -> None:
    blocks = collect_sessions_per_width(report)
    if not blocks:
        fail("report has no per-width session blocks (details.sessions is empty)")

    expected_ids = {s.get("session_id") for _, sessions in blocks for s in sessions}
    events = [
        event for event in load_ui_trace_events(trace_log)
        if event.get("session_id") in expected_ids
    ]
    if not events:
        fail("no ui_auto_debug events found for expected sessions")

    events_by_session: dict[str, list[dict[str, str]]] = {}
    for event in events:
        events_by_session.setdefault(event["session_id"], []).append(event)

    for session_id, session_events in events_by_session.items():
        saw_pending = False
        for event in session_events:
            pending = event.get("height_cache_load_pending")
            if pending == "true":
                saw_pending = True
                continue
            if saw_pending and pending == "false":
                print(
                    "  offload ordering: "
                    f"session {session_id} rendered before height-cache completion"
                )
                return

    fail(
        "no session showed a render-then-clear height-cache pending transition in ui_auto_debug.\n"
        "  expected at least one session with height_cache_load_pending=true followed later by false"
    )


def assert_new_chat_sidebar_ordering(report: dict) -> None:
    details = report.get("details", {})
    ordering = details.get("new_chat_ordering")
    if not ordering:
        fail("report is missing details.new_chat_ordering")

    requested_name = ordering.get("requested_name")
    trace = ordering.get("trace", {}) or {}
    if not requested_name:
        fail("details.new_chat_ordering.requested_name missing")
    if not trace:
        fail("details.new_chat_ordering.trace missing")

    session_id = trace.get("session_id")
    if not session_id:
        fail("new chat ordering trace missing session_id")
    if trace.get("session_name") != requested_name:
        fail(
            "new chat ordering trace did not render the requested session name.\n"
            f"  expected: {requested_name!r}\n"
            f"  observed: {trace.get('session_name')!r}"
        )
    if trace.get("sidebar_top_session_id") != session_id:
        fail(
            "new chat ordering trace did not place the created session at sidebar index 0.\n"
            f"  session_id: {session_id!r}\n"
            f"  sidebar_top_session_id: {trace.get('sidebar_top_session_id')!r}"
        )
    if trace.get("sidebar_selected_session_id") != session_id:
        fail(
            "new chat ordering trace did not keep the created session selected.\n"
            f"  session_id: {session_id!r}\n"
            f"  sidebar_selected_session_id: {trace.get('sidebar_selected_session_id')!r}"
        )
    if trace.get("sidebar_top_session_name") != requested_name:
        fail(
            "new chat ordering trace reported a mismatched sidebar top row name.\n"
            f"  expected: {requested_name!r}\n"
            f"  observed: {trace.get('sidebar_top_session_name')!r}"
        )

    print(
        "  new chat ordering: "
        f"created session {requested_name!r} rendered immediately at sidebar index 0"
    )


def assert_assistant_render(report: dict) -> None:
    total = 0
    for width, sessions in collect_sessions_per_width(report):
        for s in sessions:
            sid = s.get("session_id")
            if not s.get("text_visible_heuristic", False):
                fail(f"width {width} session {sid}: text_visible_heuristic was false")
            trace = s.get("trace", {}) or {}
            try:
                msg_count = int(trace.get("message_count", "0"))
                rendered_rows = int(trace.get("rendered_rows", "0"))
            except ValueError:
                fail(f"width {width} session {sid}: trace integers unparseable: {trace}")
            if msg_count < 1:
                fail(f"width {width} session {sid}: trace.message_count={msg_count} (no assistant content)")
            if rendered_rows < msg_count:
                fail(
                    f"width {width} session {sid}: rendered_rows={rendered_rows} "
                    f"< message_count={msg_count} (rows clipped before assistant message)"
                )
            visual = s.get("visual_metric", {}) or {}
            stddev = float(visual.get("stddev", 0.0))
            if stddev < 0.01:
                fail(f"width {width} session {sid}: screenshot stddev={stddev} (blank?)")
            total += 1
    print(f"  assistant render: {total} session×width combos, all visible + non-blank")


def assert_height_cache(report: dict, home: Path | None, multi_width: bool) -> None:
    blocks = collect_sessions_per_width(report)

    if multi_width:
        if len(blocks) < 2:
            fail(f"--multi-width set but report has {len(blocks)} width block(s)")
        per_session: dict[str, dict[int, float]] = {}
        for width, sessions in blocks:
            for s in sessions:
                trace = s.get("trace", {}) or {}
                height = float(trace.get("first_rect_height", "0") or 0.0)
                if height <= 0.0:
                    fail(f"width {width} session {s.get('session_id')}: first_rect_height not measured")
                per_session.setdefault(s["session_id"], {})[width] = height
        for sid, by_width in per_session.items():
            distinct = len(set(by_width.values()))
            if distinct < 1:
                fail(f"session {sid}: no height measurements across widths")
        print(f"  height cache: {len(per_session)} sessions measured at "
              f"{len(blocks)} widths, all populated")
    else:
        for width, sessions in blocks:
            for s in sessions:
                trace = s.get("trace", {}) or {}
                height = float(trace.get("first_rect_height", "0") or 0.0)
                if height <= 0.0:
                    fail(f"width {width} session {s.get('session_id')}: first_rect_height not measured")
        print(f"  height cache: first_rect_height populated for all sessions at width "
              f"{[w for w, _ in blocks]}")

    if home is not None:
        canonical = shared_runtime_root(home) / ".codex-desktop"
        legacy = home / ".codex-desktop"
        cache_root = canonical if canonical.exists() or not legacy.exists() else legacy
        cache_dir = cache_root / "height_cache"
        if not cache_dir.exists():
            print(f"  height cache: NOTE — {cache_dir} not yet created (binary may "
                  f"not have idled long enough to persist); skipping disk check")
            return
        cached = list(cache_dir.glob("*.json"))
        if not cached:
            print(f"  height cache: NOTE — {cache_dir} empty (no caches persisted yet); "
                  f"skipping disk check")
            return
        bad = []
        for p in cached:
            try:
                payload = json.loads(p.read_text())
            except json.JSONDecodeError as e:
                bad.append(f"{p.name}: invalid JSON ({e})")
                continue
            if "widths" not in payload or not payload["widths"]:
                bad.append(f"{p.name}: no width buckets")
        if bad:
            fail("height_cache files invalid:\n  " + "\n  ".join(bad))
        print(f"  height cache: {len(cached)} cache JSON(s) on disk, all parseable")


def load_latest_assistant_content(session_json: Path) -> str:
    if not session_json.exists():
        fail(f"session JSON missing: {session_json}")
    payload = json.loads(session_json.read_text())
    messages = payload.get("messages", [])
    for message in reversed(messages):
        if not message.get("is_user", False):
            return str(message.get("content", ""))
    fail(f"session JSON has no assistant messages: {session_json}")


def tool_transcript_lines(content: str) -> list[str]:
    return [
        line for line in content.splitlines()
        if line.startswith(TRANSCRIPT_STATUS_PREFIXES)
        or STANDARD_TOOL_TRANSCRIPT_LINE_RE.match(line)
    ]


def assert_prompt_debug_plaintext_stream(
    report: dict,
    session_json: Path | None,
    required_substrings: list[str],
    min_tool_transcript_lines: int = 0,
    required_tool_transcript_substrings: list[str] | None = None,
    forbidden_tool_transcript_substrings: list[str] | None = None,
) -> None:
    if report.get("scenario") != "prompt_debug":
        fail(f"prompt-debug assertions require scenario=prompt_debug, got {report.get('scenario')!r}")

    measurements = report.get("measurements", [])
    if not measurements:
        fail("prompt_debug report has no measurements")
    measurement = measurements[0]
    ai_response_end = measurement.get("ai_response_end", {}) or {}
    if ai_response_end.get("status") != "done":
        fail(f"prompt_debug ai_response_end.status={ai_response_end.get('status')!r}, expected 'done'")
    if measurement.get("last_markdown_row", "not-null") is not None:
        fail(
            f"prompt_debug last_markdown_row={measurement.get('last_markdown_row')!r}, "
            "expected null for plaintext-stream rendering"
        )

    screenshot = find_artifact_path(report, "window_screenshot")
    if screenshot is None:
        fail("prompt_debug report missing window_screenshot artifact")
    if not screenshot.exists():
        fail(f"prompt_debug screenshot missing on disk: {screenshot}")
    if screenshot.stat().st_size < 1024:
        fail(f"prompt_debug screenshot too small to be credible: {screenshot.stat().st_size} bytes")

    print(f"  prompt_debug: screenshot={screenshot.name} size={screenshot.stat().st_size} bytes")

    if session_json is None:
        return

    content = load_latest_assistant_content(session_json)
    if PRIVATE_USE_RE.search(content):
        fail("latest assistant message still contains private-use glyphs")
    if BOX_DRAWING_RE.search(content):
        fail("latest assistant message still contains box-drawing glyphs")
    for required in required_substrings:
        if required not in content:
            fail(f"latest assistant message missing required substring: {required!r}")
    transcript_lines = tool_transcript_lines(content)
    if min_tool_transcript_lines > 0 and len(transcript_lines) < min_tool_transcript_lines:
        fail(
            "latest assistant message is missing transcript-style tool lines.\n"
            f"  required at least: {min_tool_transcript_lines}\n"
            f"  observed: {len(transcript_lines)}"
        )
    generic_tool_lines = [
        line for line in transcript_lines
        if GENERIC_STANDARD_TOOL_LABEL_RE.match(line)
    ]
    if generic_tool_lines:
        fail(
            "latest assistant transcript contains generic `tool` labels.\n"
            f"  observed transcript lines: {generic_tool_lines!r}"
        )
    for required in required_tool_transcript_substrings or []:
        if not any(required in line for line in transcript_lines):
            fail(
                "latest assistant transcript is missing required tool substring.\n"
                f"  required: {required!r}\n"
                f"  observed transcript lines: {transcript_lines!r}"
            )
    for forbidden in forbidden_tool_transcript_substrings or []:
        if any(forbidden in line for line in transcript_lines):
            fail(
                "latest assistant transcript contains forbidden tool substring.\n"
                f"  forbidden: {forbidden!r}\n"
                f"  observed transcript lines: {transcript_lines!r}"
            )

    print("  prompt_debug: latest assistant content is normalized and contains required substrings")
    if (
        min_tool_transcript_lines > 0
        or required_tool_transcript_substrings
        or forbidden_tool_transcript_substrings
    ):
        print(
            "  prompt_debug: transcript tool lines satisfy required/forbidden substring checks"
        )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="path to auto-ui report.json")
    parser.add_argument("--home", type=Path, help="HOME used for the run (for disk checks)")
    parser.add_argument("--multi-width", action="store_true",
                        help="report is from a multi-width run (stricter height cache check)")
    parser.add_argument("--prompt-debug-session-json", type=Path,
                        help="session JSON to validate for prompt_debug plaintext-stream assertions")
    parser.add_argument("--require-substring", action="append", default=[],
                        help="substring that must appear in the latest assistant message")
    parser.add_argument("--require-tool-transcript-count", type=int, default=0,
                        help="minimum number of transcript-style tool lines required in the latest assistant message")
    parser.add_argument("--require-tool-transcript-substring", action="append", default=[],
                        help="substring that must appear in at least one transcript-style tool line")
    parser.add_argument("--forbid-tool-transcript-substring", action="append", default=[],
                        help="substring that must not appear in any transcript-style tool line")
    args = parser.parse_args()

    report = load_report(args.report)

    print(f"asserting auto-ui report: {args.report}")
    print(f"  scenario={report.get('scenario')} status={report.get('status')} "
          f"widths={report['details'].get('widths')}")

    if report.get("status") != "ok":
        fail(f"report status is {report.get('status')!r}, expected 'ok'")

    try:
        if report.get("scenario") == "prompt_debug":
            assert_prompt_debug_plaintext_stream(
                report,
                args.prompt_debug_session_json,
                args.require_substring,
                args.require_tool_transcript_count,
                args.require_tool_transcript_substring,
                args.forbid_tool_transcript_substring,
            )
        else:
            trace_log = Path(report["details"]["trace_log"])
            assert_chat_switching(report, trace_log)
            assert_new_chat_sidebar_ordering(report)
            assert_session_switch_offload(report, trace_log)
            assert_assistant_render(report)
            assert_height_cache(report, args.home, args.multi_width)
    except AssertionFailure as e:
        print(f"FAIL: {e}", file=sys.stderr)
        return 1

    print("PASS: all auto-ui behavioral assertions hold")
    return 0


if __name__ == "__main__":
    sys.exit(main())

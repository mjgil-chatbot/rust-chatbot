#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import time
from dataclasses import dataclass
from typing import Any

import auto_ui_debug as common


ROOT = pathlib.Path(__file__).resolve().parent.parent


@dataclass
class SessionDetails:
    session_id: str
    name: str
    updated_at: str
    message_count: int
    provider_session_id: str | None
    launched_from: str | None


def provider_session_field(provider: str) -> str:
    return {
        "claude": "claude_session_id",
        "codex": "codex_session_id",
        "gemini": "gemini_session_id",
        "geminiforge": "gemini_session_id",
        "geminicliforge": "gemini_session_id",
        "minimax": "minimax_session_id",
        "minimaxforge": "minimax_session_id",
        "omniforge": "omniforge_session_id",
    }[provider]


def session_file_path(provider: str, session_id: str) -> pathlib.Path:
    return common.provider_data_dir(provider) / "sessions" / f"{session_id}.json"


def load_metadata_sessions(provider: str, include_hidden: bool) -> list[dict[str, Any]]:
    metadata_path = common.provider_data_dir(provider) / "sessions.json"
    payload = json.loads(metadata_path.read_text(encoding="utf-8"))
    sessions: list[dict[str, Any]] = []
    for raw in payload.get("sessions", {}).values():
        if not include_hidden and raw.get("hidden"):
            continue
        sessions.append(raw)
    sessions.sort(key=lambda raw: raw.get("updated_at", ""), reverse=True)
    return sessions


def load_session_details(provider: str, session_id: str) -> SessionDetails:
    raw = json.loads(session_file_path(provider, session_id).read_text(encoding="utf-8"))
    messages = raw.get("messages", [])
    return SessionDetails(
        session_id=raw["id"],
        name=raw["name"],
        updated_at=raw.get("updated_at", ""),
        message_count=len(messages),
        provider_session_id=raw.get(provider_session_field(provider)),
        launched_from=raw.get("launched_from"),
    )


def resolve_session(
    provider: str,
    session_id: str | None,
    session_name: str | None,
    include_hidden: bool,
) -> SessionDetails:
    if session_id:
        return load_session_details(provider, session_id)

    sessions = load_metadata_sessions(provider, include_hidden)
    if session_name:
        for raw in sessions:
            if raw.get("name") == session_name:
                return load_session_details(provider, raw["id"])
        raise SystemExit(f"Session named {session_name!r} was not found for provider {provider!r}.")

    for raw in sessions:
        if int(raw.get("message_count", 0)) > 0:
            return load_session_details(provider, raw["id"])

    raise SystemExit(f"No visible sessions with messages were found for provider {provider!r}.")


def parse_widths(raw: str) -> list[int]:
    return [int(part.strip()) for part in raw.split(",") if part.strip()]


def build_output_dir(output_dir: str | None) -> pathlib.Path:
    if output_dir:
        path = pathlib.Path(output_dir)
    else:
        stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
        path = ROOT / "tmp" / f"auto-ui-header-debug-{stamp}"
    path.mkdir(parents=True, exist_ok=True)
    return path


def require_release_binaries() -> None:
    required = [
        common.chatbot_ctl_path(),
        ROOT / "target" / "release" / "rust-chatbot",
    ]
    missing = [path for path in required if not path.exists()]
    if not missing:
        return
    missing_text = ", ".join(str(path) for path in missing)
    raise SystemExit(
        "Missing release binaries required by this script: "
        f"{missing_text}. Build them first with the lightweight build path."
    )


def clamp(value: int, low: int, high: int) -> int:
    return max(low, min(high, value))


def crop_image(
    source: pathlib.Path,
    target: pathlib.Path,
    *,
    x: int,
    y: int,
    width: int,
    height: int,
) -> None:
    geometry = f"{width}x{height}+{x}+{y}"
    common.run(
        [
            "convert",
            str(source),
            "-crop",
            geometry,
            "+repage",
            str(target),
        ]
    )


def enhance_image(source: pathlib.Path, target: pathlib.Path) -> None:
    common.run(
        [
            "convert",
            str(source),
            "-colorspace",
            "Gray",
            "-normalize",
            "-contrast-stretch",
            "1%x1%",
            "-resize",
            "200%",
            str(target),
        ]
    )


def approximate_header_focus_crop(
    trace: dict[str, str],
    geometry: common.WindowGeometry,
    screenshot_width: int,
    screenshot_height: int,
    header_height: int,
) -> dict[str, int]:
    ppp = float(trace.get("pixels_per_point", "1") or "1")
    content_width_px = max(1, int(float(trace.get("content_width", "700") or "700") * ppp))
    first_rect_min_x_px = int(float(trace.get("first_rect_min_x", "0") or "0") * ppp)
    focus_x = first_rect_min_x_px - geometry.x - 24
    if focus_x < 0 or focus_x >= screenshot_width:
        focus_x = screenshot_width - content_width_px
    focus_x = clamp(focus_x, 0, max(0, screenshot_width - 1))
    focus_width = clamp(content_width_px, 1, screenshot_width - focus_x)
    focus_height = clamp(header_height, 1, screenshot_height)
    return {
        "x": focus_x,
        "y": 0,
        "width": focus_width,
        "height": focus_height,
    }


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Launch a session and capture header-focused UI debug artifacts."
    )
    parser.add_argument(
        "--provider",
        choices=[
            "claude",
            "codex",
            "gemini",
            "geminiforge",
            "geminicliforge",
            "minimax",
            "minimaxforge",
            "omniforge",
        ],
        default="codex",
    )
    parser.add_argument("--instance", type=int)
    parser.add_argument("--session-id")
    parser.add_argument("--session-name")
    parser.add_argument("--include-hidden", action="store_true")
    parser.add_argument("--widths", default="520,900,1000")
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--header-height", type=int, default=140)
    parser.add_argument("--window-timeout", type=float, default=15.0)
    parser.add_argument("--trace-timeout", type=float, default=8.0)
    parser.add_argument("--settle", type=float, default=0.8)
    parser.add_argument("--output-dir")
    parser.add_argument(
        "--keep-front",
        action="store_true",
        help="Do not lower the launched window behind other windows.",
    )
    args = parser.parse_args()

    if "DISPLAY" not in os.environ:
        raise SystemExit(
            "DISPLAY is not set. Run this from a desktop terminal in the same X session as Rust Chatbot."
        )

    widths = parse_widths(args.widths)
    if not widths:
        raise SystemExit("At least one width is required.")

    require_release_binaries()
    session = resolve_session(args.provider, args.session_id, args.session_name, args.include_hidden)
    title = common.provider_title(args.provider, args.instance)
    output_dir = build_output_dir(args.output_dir)
    progress_path = output_dir / "progress.log"
    log_path = common.newest_trace_log()
    startup_offset = log_path.stat().st_size

    common.log_line(f"output_dir={output_dir}", progress_path)
    common.log_line(f"trace_log={log_path}", progress_path)
    common.log_line(f"session_id={session.session_id}", progress_path)
    common.log_line(f"session_name={session.name}", progress_path)
    common.log_line(
        f"provider_session_id={session.provider_session_id or '<none>'}",
        progress_path,
    )
    common.log_line(
        f"launched_from={session.launched_from or '<none>'}",
        progress_path,
    )

    launched_pid: int | None = None
    try:
        launched_pid, window_id = common.launch_targeted_session_window(
            provider=args.provider,
            instance=args.instance,
            title=title,
            session_id=session.session_id,
            keep_front=args.keep_front,
            window_timeout=args.window_timeout,
            progress_path=progress_path,
        )

        current_offset, startup_trace, startup_code_blocks = common.wait_for_trace_bundle(
            log_path,
            startup_offset,
            session_id=session.session_id,
            timeout_s=max(args.trace_timeout, 10.0),
        )
        common.log_line(
            "startup trace captured "
            f"content_width={startup_trace.get('content_width', 'n/a')} "
            f"first_rect_min_x={startup_trace.get('first_rect_min_x', 'n/a')}",
            progress_path,
        )

        report: dict[str, Any] = {
            "provider": args.provider,
            "instance": args.instance,
            "title_substring": title,
            "window_id": window_id,
            "trace_log": str(log_path),
            "output_dir": str(output_dir),
            "session": {
                "session_id": session.session_id,
                "name": session.name,
                "updated_at": session.updated_at,
                "message_count": session.message_count,
                "provider_session_id": session.provider_session_id,
                "launched_from": session.launched_from,
            },
            "startup_trace": startup_trace,
            "startup_code_block_traces": startup_code_blocks,
            "widths": [],
            "ocr_available": False,
        }

        for width in widths:
            common.log_line(f"resizing window to width={width} height={args.height}", progress_path)
            geometry = common.resize_window(window_id, width, args.height)
            if not args.keep_front:
                common.lower_window(window_id)
            time.sleep(args.settle)

            try:
                current_offset, trace, code_block_traces = common.wait_for_trace_bundle(
                    log_path,
                    current_offset,
                    session_id=session.session_id,
                    timeout_s=args.trace_timeout,
                )
            except TimeoutError:
                trace = startup_trace
                code_block_traces = startup_code_blocks
                common.log_line(
                    f"resize produced no new ui trace for width={width}; reusing startup trace",
                    progress_path,
                )

            screenshot_path = output_dir / f"{args.provider}-w{width}-{session.session_id[:8]}-window.png"
            top_strip_path = output_dir / f"{args.provider}-w{width}-{session.session_id[:8]}-top-strip.png"
            focus_path = output_dir / f"{args.provider}-w{width}-{session.session_id[:8]}-header-focus.png"
            focus_enhanced_path = (
                output_dir / f"{args.provider}-w{width}-{session.session_id[:8]}-header-focus-enhanced.png"
            )

            common.capture_window_screenshot(window_id, screenshot_path)
            screenshot_width, screenshot_height = common.image_size(screenshot_path)
            top_strip_height = clamp(args.header_height, 1, screenshot_height)
            crop_image(
                screenshot_path,
                top_strip_path,
                x=0,
                y=0,
                width=screenshot_width,
                height=top_strip_height,
            )

            focus_crop = approximate_header_focus_crop(
                trace,
                geometry,
                screenshot_width,
                screenshot_height,
                args.header_height,
            )
            crop_image(screenshot_path, focus_path, **focus_crop)
            enhance_image(focus_path, focus_enhanced_path)

            top_strip_metric = common.crop_metric(
                screenshot_path,
                0,
                0,
                screenshot_width,
                top_strip_height,
            )
            focus_metric = common.crop_metric(
                screenshot_path,
                focus_crop["x"],
                focus_crop["y"],
                focus_crop["width"],
                focus_crop["height"],
            )

            report["widths"].append(
                {
                    "requested_width": width,
                    "window_geometry": geometry.__dict__,
                    "trace": trace,
                    "code_block_traces": code_block_traces,
                    "screenshot": str(screenshot_path),
                    "top_strip": {
                        "path": str(top_strip_path),
                        "height": top_strip_height,
                        "metric": top_strip_metric,
                    },
                    "header_focus": {
                        "path": str(focus_path),
                        "enhanced_path": str(focus_enhanced_path),
                        "crop": focus_crop,
                        "metric": focus_metric,
                    },
                }
            )
            common.log_line(
                "captured "
                f"width={width} "
                f"focus_crop=({focus_crop['x']},{focus_crop['y']},{focus_crop['width']},{focus_crop['height']}) "
                f"focus_stddev={focus_metric['stddev']:.4f}",
                progress_path,
            )

        report_path = output_dir / "report.json"
        report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")

        print(f"wrote {report_path}")
        print("\nexpected header values:")
        print(f"  session name: {session.name}")
        print(f"  provider session id: {session.provider_session_id or '<none>'}")
        print(f"  launched_from: {session.launched_from or '<none>'}")
        print("\nartifacts:")
        print(f"  progress: {progress_path}")
        print(f"  report: {report_path}")
        for entry in report["widths"]:
            print(
                f"  width {entry['requested_width']}: "
                f"window={entry['screenshot']} "
                f"header={entry['header_focus']['path']} "
                f"enhanced={entry['header_focus']['enhanced_path']}"
            )
        return 0
    finally:
        if launched_pid is not None:
            common.stop_chatbot_pid(launched_pid)
            common.wait_for_pid_exit(launched_pid, args.window_timeout)


if __name__ == "__main__":
    raise SystemExit(main())

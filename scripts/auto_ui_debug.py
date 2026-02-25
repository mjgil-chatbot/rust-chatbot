#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import pathlib
import time
from typing import Any

from auto_ui_debug_support import *  # noqa: F401,F403

NEW_CHAT_ORDERING_NAME = "auto-ui-ordering-check"


def collect_new_chat_ordering(
    *,
    provider: str,
    instance: int | None,
    title: str,
    seed_session: SessionEntry,
    width: int,
    height: int,
    keep_front: bool,
    window_timeout: float,
    trace_timeout: float,
    settle: float,
    log_path: pathlib.Path,
    progress_path: pathlib.Path,
    output_dir: pathlib.Path,
) -> dict[str, Any]:
    startup_log_offset = log_path.stat().st_size
    launched_pid, window_id = launch_targeted_session_window(
        provider=provider,
        instance=instance,
        title=title,
        session_id=seed_session.session_id,
        keep_front=keep_front,
        window_timeout=window_timeout,
        progress_path=progress_path,
    )

    try:
        requested_name = f"{NEW_CHAT_ORDERING_NAME}-{int(time.time())}"
        wait_for_trace_bundle(
            log_path,
            startup_log_offset,
            session_id=seed_session.session_id,
            timeout_s=max(trace_timeout, 10.0),
        )

        geometry = resize_window(window_id, width, height)
        if not keep_front:
            lower_window(window_id)
        time.sleep(settle)

        create_log_offset = log_path.stat().st_size
        log_line(
            f"creating new chat ordering probe name={requested_name} width={width}",
            progress_path,
        )
        create_new_chat(window_id, requested_name, settle)
        _, trace, _ = wait_for_trace_bundle_by_session_name(
            log_path,
            create_log_offset,
            session_name=requested_name,
            timeout_s=max(trace_timeout, 10.0),
        )

        screenshot_path = output_dir / f"{provider}-new-chat-ordering.png"
        capture_window_screenshot(window_id, screenshot_path)

        return {
            "requested_name": requested_name,
            "seed_session_id": seed_session.session_id,
            "seed_session_name": seed_session.name,
            "window_geometry": geometry.__dict__,
            "trace": trace,
            "screenshot": str(screenshot_path),
        }
    finally:
        stop_chatbot_pid(launched_pid)
        wait_for_pid_exit(launched_pid, window_timeout)


def main() -> int:
    parser = argparse.ArgumentParser(description="Cycle chatbot sessions and collect UI width traces.")
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
    parser.add_argument("--widths", default=",".join(str(width) for width in DEFAULT_WIDTHS))
    parser.add_argument("--height", type=int, default=900)
    parser.add_argument("--max-sessions", type=int, default=8)
    parser.add_argument("--session-id")
    parser.add_argument("--include-hidden", action="store_true")
    parser.add_argument(
        "--launch",
        dest="launch_if_missing",
        action="store_true",
        help="Launch a fresh app window with RUST_CHATBOT_AUTO_UI_DEBUG=1 and use that window.",
    )
    parser.add_argument(
        "--no-launch",
        dest="launch_if_missing",
        action="store_false",
        help="Reuse an existing matching window instead of launching a fresh one.",
    )
    parser.add_argument("--window-timeout", type=float, default=15.0)
    parser.add_argument("--trace-timeout", type=float, default=5.0)
    parser.add_argument("--settle", type=float, default=0.7)
    parser.add_argument("--output-dir")
    parser.add_argument(
        "--keep-front",
        action="store_true",
        help="Do not lower launched automation windows behind other windows.",
    )
    parser.set_defaults(launch_if_missing=True)
    args = parser.parse_args()

    if "DISPLAY" not in os.environ:
        raise SystemExit(
            "DISPLAY is not set. Run this script from a desktop terminal in the same X session as Rust Chatbot."
        )

    widths = parse_widths(args.widths)
    if args.session_id:
        sessions = [load_session_by_id(args.provider, args.session_id)]
    else:
        sessions = load_sessions(args.provider, args.max_sessions, args.include_hidden)
    if not sessions:
        raise SystemExit("No visible sessions with messages were found for that provider.")

    title = provider_title(args.provider, args.instance)
    output_dir = build_output_dir(args.output_dir)
    progress_path = output_dir / "progress.log"
    log_line(f"output_dir={output_dir}", progress_path)
    log_path = newest_trace_log()
    log_offset = log_path.stat().st_size
    log_line(f"trace_log={log_path}", progress_path)
    per_session_launch_mode = args.launch_if_missing and not args.session_id
    window_id: str | None = None
    interaction_window_id: str | None = None

    if not per_session_launch_mode:
        if args.launch_if_missing:
            existing_pids = list_chatbot_pids()
            existing_window_ids = set(find_window_ids(title))
            existing_interaction_window_ids = set(find_interaction_window_ids(title))
            log_line(f"launching fresh {title} with RUST_CHATBOT_AUTO_UI_DEBUG=1", progress_path)
            launch_window(args.provider, args.instance, args.session_id)
            launched_pid = wait_for_new_pid(existing_pids, args.window_timeout)
            if launched_pid is not None:
                log_line(f"detected launched_pid={launched_pid}", progress_path)
                window_id = find_window_id_for_pid(launched_pid, args.window_timeout)
                interaction_window_id = find_interaction_window_id_for_pid(
                    launched_pid,
                    args.window_timeout,
                )

            if window_id is None:
                window_id = wait_for_new_window_id(title, existing_window_ids, args.window_timeout)
            if window_id is None:
                raise SystemExit(f"Could not find a new window matching {title!r} after launch.")
            if interaction_window_id is None:
                interaction_window_id = wait_for_new_interaction_window_id(
                    title,
                    existing_interaction_window_ids,
                    args.window_timeout,
                )
            if not args.keep_front:
                lower_window(window_id)
        else:
            window_id = find_window_id(title, args.window_timeout)
            if window_id is None:
                raise SystemExit(
                    f"No window matching {title!r} was found. Re-run without --no-launch to let the script start one."
                )
            interaction_window_id = find_interaction_window_id(title, args.window_timeout)

        if interaction_window_id is None:
            interaction_window_id = window_id

        log_line(f"using window_id={window_id}", progress_path)
        log_line(f"using interaction_window_id={interaction_window_id}", progress_path)

    report: dict[str, Any] = {
        "provider": args.provider,
        "instance": args.instance,
        "window_id": window_id,
        "title_substring": title,
        "widths": widths,
        "height": args.height,
        "sessions": [],
        "output_dir": str(output_dir),
        "trace_log": str(log_path),
    }

    for width in widths:
        width_entry: dict[str, Any] = {
            "requested_width": width,
            "window_geometry": None,
            "sessions": [],
        }

        for session in sessions:
            launched_pid: int | None = None
            current_window_id = window_id
            current_log_offset = log_offset
            geometry: WindowGeometry | None = None
            startup_trace: dict[str, str] | None = None
            startup_code_block_traces: list[dict[str, str]] = []

            try:
                if per_session_launch_mode:
                    startup_log_offset = log_path.stat().st_size
                    launched_pid, current_window_id = launch_targeted_session_window(
                        provider=args.provider,
                        instance=args.instance,
                        title=title,
                        session_id=session.session_id,
                        keep_front=args.keep_front,
                        window_timeout=args.window_timeout,
                        progress_path=progress_path,
                    )
                    current_log_offset = log_path.stat().st_size
                    log_line(
                        f"observing startup session={session.name} messages={session.message_count} width={width}",
                        progress_path,
                    )
                    startup_log_offset, startup_trace, startup_code_block_traces = wait_for_trace_bundle(
                        log_path,
                        startup_log_offset,
                        session_id=session.session_id,
                        timeout_s=max(args.trace_timeout, 10.0),
                    )
                    current_log_offset = startup_log_offset
                elif args.session_id:
                    log_line(
                        f"observing startup session={session.name} messages={session.message_count} width={width}",
                        progress_path,
                    )
                else:
                    log_line(
                        f"selecting session={session.name} messages={session.message_count} width={width}",
                        progress_path,
                    )
                    select_session(interaction_window_id, session.name)
                    time.sleep(args.settle)

                if current_window_id is None:
                    raise SystemExit("No window is available for capture.")

                log_line(f"resizing window to width={width} height={args.height}", progress_path)
                geometry = resize_window(current_window_id, width, args.height)
                if args.launch_if_missing and not args.keep_front:
                    lower_window(current_window_id)
                time.sleep(args.settle)

                try:
                    log_offset, trace, code_block_traces = wait_for_trace_bundle(
                        log_path,
                        current_log_offset,
                        session_id=session.session_id,
                        timeout_s=args.trace_timeout,
                    )
                except TimeoutError:
                    if startup_trace is None:
                        raise
                    trace = startup_trace
                    code_block_traces = startup_code_block_traces
                    log_line(
                        f"resize produced no new ui trace for session={session.name}; reusing startup trace",
                        progress_path,
                    )

                screenshot_path = output_dir / f"{args.provider}-w{width}-{session.session_id[:8]}.png"
                log_line(f"capturing screenshot={screenshot_path.name}", progress_path)
                capture_window_screenshot(current_window_id, screenshot_path)

                ppp = float(trace.get("pixels_per_point", "1") or "1")
                screenshot_width, screenshot_height = image_size(screenshot_path)
                crop_x = max(0, int(float(trace.get("first_rect_min_x", "0")) * ppp) - geometry.x)
                crop_y = max(0, int(float(trace.get("first_rect_min_y", "0")) * ppp) - geometry.y)
                max_crop_width = max(1, screenshot_width - crop_x)
                max_crop_height = max(1, screenshot_height - crop_y)
                crop_width = min(
                    max_crop_width,
                    max(1, int(float(trace.get("first_rect_width", "0")) * ppp)),
                )
                crop_height = min(
                    max_crop_height,
                    max(1, int(float(trace.get("first_rect_height", "0")) * ppp)),
                )
                metric = crop_metric(screenshot_path, crop_x, crop_y, crop_width, crop_height)

                width_entry["sessions"].append(
                    {
                        "session_id": session.session_id,
                        "session_name": session.name,
                        "message_count": session.message_count,
                        "window_geometry": geometry.__dict__ if geometry is not None else None,
                        "trace": trace,
                        "code_block_traces": code_block_traces,
                        "screenshot": str(screenshot_path),
                        "crop": {
                            "x": crop_x,
                            "y": crop_y,
                            "width": crop_width,
                            "height": crop_height,
                        },
                        "visual_metric": metric,
                        "text_visible_heuristic": heuristic_text_visible(metric),
                    }
                )
                log_line(
                    "captured "
                    f"area_width={trace.get('message_area_available_width', 'n/a')} "
                    f"overflow={trace.get('max_rendered_overflow', 'n/a')} "
                    f"code_blocks={len(code_block_traces)} "
                    f"text_visible={heuristic_text_visible(metric)}",
                    progress_path,
                )
            except TimeoutError as exc:
                log_line(f"trace timeout for session={session.name}: {exc}", progress_path)
                width_entry["sessions"].append(
                    {
                        "session_id": session.session_id,
                        "session_name": session.name,
                        "error": (
                            f"{exc}. If you attached to an already-open window, it was probably not "
                            "started with RUST_CHATBOT_AUTO_UI_DEBUG=1."
                        ),
                    }
                )
            finally:
                if launched_pid is not None:
                    stop_chatbot_pid(launched_pid)
                    wait_for_pid_exit(launched_pid, args.window_timeout)

        report["sessions"].append(width_entry)

    if not args.session_id:
        report["new_chat_ordering"] = collect_new_chat_ordering(
            provider=args.provider,
            instance=args.instance,
            title=title,
            seed_session=sessions[0],
            width=widths[0],
            height=args.height,
            keep_front=args.keep_front,
            window_timeout=args.window_timeout,
            trace_timeout=args.trace_timeout,
            settle=args.settle,
            log_path=log_path,
            progress_path=progress_path,
            output_dir=output_dir,
        )

    report_path = output_dir / "report.json"
    report_path.write_text(json.dumps(report, indent=2), encoding="utf-8")

    print(f"wrote {report_path}")
    for width_entry in report["sessions"]:
        print(f"\nwidth {width_entry['requested_width']}:")
        for entry in width_entry["sessions"]:
            if "error" in entry:
                print(f"  {entry['session_name']}: ERROR {entry['error']}")
                continue
            overflow = entry["trace"].get("max_rendered_overflow", "n/a")
            text_ok = "yes" if entry["text_visible_heuristic"] else "no"
            area_width = entry["trace"].get("message_area_available_width", "n/a")
            code_block_count = len(entry.get("code_block_traces", []))
            print(
                f"  {entry['session_name']}: area_width={area_width} overflow={overflow} code_blocks={code_block_count} text_visible={text_ok}"
            )

    print("\nlogs:")
    print(f"  progress: {progress_path}")
    print(f"  trace: {log_path}")
    print(f"  report: {report_path}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())

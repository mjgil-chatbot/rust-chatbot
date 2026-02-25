#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import re
import shlex
import subprocess
import sys
import time
from dataclasses import dataclass
from typing import Any


ROOT = pathlib.Path(__file__).resolve().parent.parent
DEFAULT_WIDTHS = [520, 900, 1000]
DEFAULT_SESSION_NAMES = [
    "pl-update",
    "pl-enhance",
    "pl-24",
    "pl-assess",
    "da-scrape-result-submission",
    "pl-graph-problem",
    "pl-enhancements-2",
]
FIELD_RE = re.compile(r"(\w+)=((?:\"[^\"]*\")|(?:\S+))")
BUILD_INPUT_NAMES = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "build.rs"}
NEW_CHAT_BUTTON_X = 84
NEW_CHAT_BUTTON_Y = 74


@dataclass
class SessionEntry:
    session_id: str
    name: str
    updated_at: str
    message_count: int


@dataclass
class WindowGeometry:
    x: int
    y: int
    width: int
    height: int


def run(cmd: list[str], *, env: dict[str, str] | None = None, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, check=check, text=True, capture_output=True, env=env)


def log_line(message: str, progress_path: pathlib.Path | None = None) -> None:
    print(message, flush=True)
    if progress_path is not None:
        with progress_path.open("a", encoding="utf-8") as fh:
            fh.write(message)
            fh.write("\n")


def provider_title(provider: str, instance: int | None) -> str:
    base = {
        "claude": "Claude Rust Chatbot",
        "codex": "Codex Rust Chatbot",
        "gemini": "Gemini Rust Chatbot",
        "geminiforge": "GeminiForge Rust Chatbot",
        "geminicliforge": "GeminiCliForge Rust Chatbot",
        "minimax": "MiniMax Rust Chatbot",
        "minimaxforge": "MiniMaxForge Rust Chatbot",
        "omniforge": "OmniForge Rust Chatbot",
    }[provider]
    dir_name = ROOT.name
    if dir_name:
        return f"{base} ~ /{dir_name}"
    return base


def shared_runtime_root(home: pathlib.Path) -> pathlib.Path:
    override = os.environ.get("RUST_CHATBOT_SHARED_ROOT") or os.environ.get("RUST_CHATBOT_CONFIG_HOME")
    return pathlib.Path(override) if override else home / ".rust-chatbot"


def provider_data_dir(provider: str) -> pathlib.Path:
    home = pathlib.Path.home()
    relative = {
        "claude": home / ".claude-desktop",
        "codex": home / ".codex-desktop",
        "gemini": home / ".gemini-desktop",
        "geminiforge": home / ".gemini-forge-desktop",
        "geminicliforge": home / ".gemini-cli-forge-desktop",
        "minimax": home / ".minimax-desktop",
        "minimaxforge": home / ".minimax-forge-desktop",
        "omniforge": home / ".omniforge-desktop",
    }[provider].name
    canonical = shared_runtime_root(home) / relative
    legacy = home / relative
    if canonical.exists() or not legacy.exists():
        return canonical
    return legacy


def rust_chatbot_log_dir() -> pathlib.Path:
    xdg_state = os.environ.get("XDG_STATE_HOME")
    if xdg_state:
        return pathlib.Path(xdg_state) / "rust-chatbot"
    return pathlib.Path.home() / ".local" / "state" / "rust-chatbot"


def newest_trace_log() -> pathlib.Path:
    log_dir = rust_chatbot_log_dir()
    candidates = sorted(log_dir.glob("rust-chatbot.log.*"))
    if not candidates:
        raise SystemExit(f"No rust-chatbot tracing log found in {log_dir}")
    return candidates[-1]


def parse_bool(value: str) -> bool:
    return value.lower() in {"1", "true", "yes", "on"}


def load_sessions(provider: str, max_sessions: int, include_hidden: bool) -> list[SessionEntry]:
    metadata_path = provider_data_dir(provider) / "sessions.json"
    payload = json.loads(metadata_path.read_text(encoding="utf-8"))
    sessions = []
    seen_names: set[str] = set()
    for raw in payload.get("sessions", {}).values():
        if not include_hidden and raw.get("hidden"):
            continue
        if raw.get("message_count", 0) <= 0:
            continue
        name = raw["name"]
        if name in seen_names:
            continue
        seen_names.add(name)
        sessions.append(
            SessionEntry(
                session_id=raw["id"],
                name=name,
                updated_at=raw.get("updated_at", ""),
                message_count=int(raw.get("message_count", 0)),
            )
        )
    if DEFAULT_SESSION_NAMES:
        sessions_by_name = {session.name: session for session in sessions}
        missing_names = [name for name in DEFAULT_SESSION_NAMES if name not in sessions_by_name]
        if missing_names:
            missing = ", ".join(missing_names)
            raise SystemExit(f"Default auto-ui sessions were not found: {missing}")
        return [sessions_by_name[name] for name in DEFAULT_SESSION_NAMES]

    sessions.sort(key=lambda item: item.updated_at, reverse=True)
    return sessions[:max_sessions]


def load_session_by_id(provider: str, session_id: str) -> SessionEntry:
    metadata_path = provider_data_dir(provider) / "sessions.json"
    payload = json.loads(metadata_path.read_text(encoding="utf-8"))
    raw = payload.get("sessions", {}).get(session_id)
    if raw is None:
        raise SystemExit(f"Session {session_id!r} was not found for provider {provider!r}.")
    return SessionEntry(
        session_id=raw["id"],
        name=raw["name"],
        updated_at=raw.get("updated_at", ""),
        message_count=int(raw.get("message_count", 0)),
    )

def chatbot_ctl_path() -> pathlib.Path:
    return ROOT / "target" / "release" / "chatbot-ctl"


def adapter_repo_root(app_root: pathlib.Path = ROOT) -> pathlib.Path:
    override = os.environ.get("RUST_CHATBOT_ADAPTER_ROOT")
    if override:
        return pathlib.Path(override)
    return app_root.parent / "chatbot-adapter"


def adapter_target_dir(app_root: pathlib.Path = ROOT) -> pathlib.Path:
    override = os.environ.get("RUST_CHATBOT_ADAPTER_TARGET_DIR")
    if override:
        return pathlib.Path(override)
    return adapter_repo_root(app_root) / "target"


def release_stack_outputs(app_root: pathlib.Path = ROOT) -> list[pathlib.Path]:
    release_dir = app_root / "target" / "release"
    return [
        release_dir / "chatbot-ctl",
        release_dir / "rust-chatbot",
        release_dir / "rcc",
        release_dir / "chatbot-worker",
        adapter_target_dir(app_root) / "release" / "agent-adapter-cli",
    ]


def build_input_roots(app_root: pathlib.Path = ROOT) -> list[pathlib.Path]:
    roots = [
        app_root / "src",
        app_root / "Cargo.toml",
        app_root / "Cargo.lock",
        app_root / "rust-toolchain.toml",
        app_root / "build.rs",
    ]
    adapter_root = adapter_repo_root(app_root)
    roots.extend(
        [
            adapter_root / "crates",
            adapter_root / "Cargo.toml",
            adapter_root / "Cargo.lock",
            adapter_root / "rust-toolchain.toml",
            adapter_root / "build.rs",
        ]
    )
    return roots


def relevant_build_input(path: pathlib.Path) -> bool:
    return path.suffix == ".rs" or path.name in BUILD_INPUT_NAMES


def newest_build_input_mtime(roots: list[pathlib.Path]) -> float:
    newest = 0.0
    for root in roots:
        if not root.exists():
            continue
        if root.is_file():
            if relevant_build_input(root):
                newest = max(newest, root.stat().st_mtime)
            continue
        for path in root.rglob("*"):
            if path.is_file() and relevant_build_input(path):
                newest = max(newest, path.stat().st_mtime)
    return newest


def release_stack_is_stale(app_root: pathlib.Path = ROOT) -> bool:
    outputs = release_stack_outputs(app_root)
    if any(not path.exists() for path in outputs):
        return True

    newest_input = newest_build_input_mtime(build_input_roots(app_root))
    oldest_output = min(path.stat().st_mtime for path in outputs)
    return newest_input > oldest_output


def ensure_release_stack_current(app_root: pathlib.Path = ROOT) -> None:
    if not release_stack_is_stale(app_root):
        return

    subprocess.run(
        [
            "bash",
            "-lc",
            "source ./scripts/common.sh; rust_chatbot_build_release \"$PWD\"",
        ],
        cwd=app_root,
        check=True,
    )


def list_chatbot_pids() -> set[int]:
    ctl = chatbot_ctl_path()
    ensure_release_stack_current()
    proc = run([str(ctl), "pids"], check=False)
    pids: set[int] = set()
    for line in proc.stdout.splitlines():
        line = line.strip()
        if not line:
            continue
        try:
            pids.add(int(line))
        except ValueError:
            continue
    return pids


def launch_window(provider: str, instance: int | None, start_session_id: str | None = None) -> None:
    ctl = chatbot_ctl_path()
    ensure_release_stack_current()
    env = os.environ.copy()
    env["RUST_CHATBOT_AUTO_UI_DEBUG"] = "1"
    env["RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT"] = "1"
    if start_session_id:
        env["RUST_CHATBOT_START_SESSION_ID"] = start_session_id
    cmd = [str(ctl), "launch", "--provider", provider]
    if instance is not None:
        cmd.extend(["--instance", str(instance)])
    subprocess.run(cmd, cwd=ROOT, env=env, check=True)


def find_interaction_window_ids(title_substring: str) -> list[str]:
    proc = run(["xdotool", "search", "--name", re.escape(title_substring)], check=False)
    return [line.strip() for line in proc.stdout.splitlines() if line.strip()]


def find_interaction_window_ids_for_pid(pid: int) -> list[str]:
    proc = run(["xdotool", "search", "--pid", str(pid)], check=False)
    return [line.strip() for line in proc.stdout.splitlines() if line.strip()]


def find_window_ids(title_substring: str) -> list[str]:
    proc = run(["wmctrl", "-l"], check=False)
    window_ids: list[str] = []
    for line in proc.stdout.splitlines():
        parts = line.split(None, 3)
        if len(parts) < 4:
            continue
        hex_id = parts[0]
        title = parts[3]
        if title_substring not in title:
            continue
        window_ids.append(str(int(hex_id, 16)))
    return window_ids


def find_window_ids_for_pid(pid: int) -> list[str]:
    proc = run(["wmctrl", "-lp"], check=False)
    window_ids: list[str] = []
    for line in proc.stdout.splitlines():
        parts = line.split(None, 4)
        if len(parts) < 4:
            continue
        if parts[2] != str(pid):
            continue
        window_ids.append(str(int(parts[0], 16)))
    return window_ids


def find_interaction_window_id(title_substring: str, timeout_s: float) -> str | None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        ids = find_interaction_window_ids(title_substring)
        if ids:
            return ids[-1]
        time.sleep(0.2)
    return None


def find_interaction_window_id_for_pid(pid: int, timeout_s: float) -> str | None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        ids = find_interaction_window_ids_for_pid(pid)
        if ids:
            return ids[-1]
        time.sleep(0.2)
    return None


def find_window_id(title_substring: str, timeout_s: float) -> str | None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        ids = find_window_ids(title_substring)
        if ids:
            return ids[-1]
        time.sleep(0.2)
    return None


def find_window_id_for_pid(pid: int, timeout_s: float) -> str | None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        ids = find_window_ids_for_pid(pid)
        if ids:
            return ids[-1]
        time.sleep(0.2)
    return None


def wait_for_new_pid(before_pids: set[int], timeout_s: float) -> int | None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        current = list_chatbot_pids()
        new_pids = sorted(current - before_pids)
        if new_pids:
            return new_pids[-1]
        time.sleep(0.2)
    return None


def stop_chatbot_pid(pid: int) -> None:
    ctl = chatbot_ctl_path()
    run([str(ctl), "stop", str(pid)], check=False)


def wait_for_pid_exit(pid: int, timeout_s: float) -> None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        if pid not in list_chatbot_pids():
            return
        time.sleep(0.2)


def launch_targeted_session_window(
    *,
    provider: str,
    instance: int | None,
    title: str,
    session_id: str,
    keep_front: bool,
    window_timeout: float,
    progress_path: pathlib.Path | None,
) -> tuple[int, str]:
    existing_pids = list_chatbot_pids()
    existing_window_ids = set(find_window_ids(title))
    log_line(
        f"launching fresh {title} with RUST_CHATBOT_AUTO_UI_DEBUG=1 session_id={session_id}",
        progress_path,
    )
    launch_window(provider, instance, session_id)
    launched_pid = wait_for_new_pid(existing_pids, window_timeout)
    if launched_pid is None:
        raise SystemExit(f"Could not detect a newly launched PID for {title!r}.")
    log_line(f"detected launched_pid={launched_pid}", progress_path)
    window_id = find_window_id_for_pid(launched_pid, window_timeout)
    if window_id is None:
        window_id = wait_for_new_window_id(title, existing_window_ids, window_timeout)
    if window_id is None:
        raise SystemExit(f"Could not find a new window matching {title!r} after launch.")
    if not keep_front:
        lower_window(window_id)
    log_line(f"using window_id={window_id}", progress_path)
    return launched_pid, window_id


def wait_for_new_interaction_window_id(
    title_substring: str,
    before_ids: set[str],
    timeout_s: float,
) -> str | None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        ids = find_interaction_window_ids(title_substring)
        for window_id in ids:
            if window_id not in before_ids:
                return window_id
        time.sleep(0.2)
    return None


def wait_for_new_window_id(title_substring: str, before_ids: set[str], timeout_s: float) -> str | None:
    deadline = time.time() + timeout_s
    while time.time() < deadline:
        ids = find_window_ids(title_substring)
        for window_id in ids:
            if window_id not in before_ids:
                return window_id
        time.sleep(0.2)
    return None


def get_window_geometry(window_id: str) -> WindowGeometry:
    proc = run(["xdotool", "getwindowgeometry", "--shell", window_id])
    values: dict[str, int] = {}
    for line in proc.stdout.splitlines():
        if "=" not in line:
            continue
        key, value = line.split("=", 1)
        if key in {"X", "Y", "WIDTH", "HEIGHT"}:
            values[key] = int(value)
    return WindowGeometry(
        x=values["X"],
        y=values["Y"],
        width=values["WIDTH"],
        height=values["HEIGHT"],
    )


def activate_window(window_id: str) -> None:
    run(["xdotool", "windowactivate", "--sync", window_id])


def lower_window(window_id: str) -> None:
    run(["wmctrl", "-i", "-r", window_id, "-b", "remove,above"], check=False)
    run(["wmctrl", "-i", "-r", window_id, "-b", "add,below"], check=False)


def resize_window(window_id: str, width: int, height: int) -> WindowGeometry:
    run(["xdotool", "windowsize", "--sync", window_id, str(width), str(height)])
    return get_window_geometry(window_id)


def window_click(window_id: str, x: int, y: int) -> None:
    activate_window(window_id)
    relative = run(
        ["xdotool", "mousemove", "--sync", "--window", window_id, str(x), str(y), "click", "1"],
        check=False,
    )
    if relative.returncode == 0:
        return

    geometry = get_window_geometry(window_id)
    run(
        [
            "xdotool",
            "mousemove",
            "--sync",
            str(geometry.x + x),
            str(geometry.y + y),
            "click",
            "1",
        ]
    )


def create_new_chat(window_id: str, name: str, settle_s: float) -> None:
    window_click(window_id, NEW_CHAT_BUTTON_X, NEW_CHAT_BUTTON_Y)
    time.sleep(0.2)

    geometry = get_window_geometry(window_id)
    window_click(window_id, geometry.width // 2, max(40, geometry.height // 2 - 18))
    time.sleep(0.1)

    clear_search(window_id)
    type_text(window_id, name)
    time.sleep(0.1)
    press_key(window_id, "Return")
    time.sleep(settle_s)


def press_key(window_id: str, key: str) -> None:
    run(["xdotool", "key", "--window", window_id, "--clearmodifiers", key])


def clear_search(window_id: str) -> None:
    press_key(window_id, "ctrl+a")
    press_key(window_id, "BackSpace")


def type_text(window_id: str, text: str) -> None:
    run(["xdotool", "type", "--window", window_id, "--delay", "1", "--clearmodifiers", text])


def select_session(window_id: str, session_name: str) -> None:
    press_key(window_id, "ctrl+f")
    time.sleep(0.1)
    clear_search(window_id)
    type_text(window_id, session_name)
    time.sleep(0.1)
    press_key(window_id, "Return")


def read_new_lines(log_path: pathlib.Path, offset: int) -> tuple[int, list[str]]:
    with log_path.open("r", encoding="utf-8", errors="replace") as fh:
        fh.seek(offset)
        data = fh.read()
        new_offset = fh.tell()
    return new_offset, data.splitlines()


def parse_trace_fields(line: str) -> dict[str, str]:
    fields: dict[str, str] = {}
    for key, value in FIELD_RE.findall(line):
        if value.startswith('"') and value.endswith('"'):
            value = value[1:-1]
        fields[key] = value
    return fields


def wait_for_ui_trace(
    log_path: pathlib.Path,
    offset: int,
    *,
    session_id: str,
    timeout_s: float,
) -> tuple[int, dict[str, str]]:
    deadline = time.time() + timeout_s
    current_offset = offset
    last_match: dict[str, str] | None = None
    while time.time() < deadline:
        current_offset, lines = read_new_lines(log_path, current_offset)
        for line in lines:
            if "ui_auto_debug" not in line or f"session_id={session_id}" not in line:
                continue
            last_match = parse_trace_fields(line)
        if last_match:
            return current_offset, last_match
        time.sleep(0.15)
    raise TimeoutError(f"No ui_auto_debug trace observed for session {session_id}")


def wait_for_trace_bundle(
    log_path: pathlib.Path,
    offset: int,
    *,
    session_id: str,
    timeout_s: float,
    quiet_s: float = 0.5,
) -> tuple[int, dict[str, str], list[dict[str, str]]]:
    deadline = time.time() + timeout_s
    quiet_deadline: float | None = None
    current_offset = offset
    last_ui_match: dict[str, str] | None = None
    code_blocks: list[dict[str, str]] = []

    while time.time() < deadline:
        current_offset, lines = read_new_lines(log_path, current_offset)
        saw_new_relevant_line = False
        for line in lines:
            if "ui_auto_debug_code_block" in line:
                code_blocks.append(parse_trace_fields(line))
                saw_new_relevant_line = True
                continue
            if "ui_auto_debug" in line and f"session_id={session_id}" in line:
                last_ui_match = parse_trace_fields(line)
                saw_new_relevant_line = True

        if last_ui_match is not None:
            if saw_new_relevant_line:
                quiet_deadline = time.time() + quiet_s
            elif quiet_deadline is not None and time.time() >= quiet_deadline:
                return current_offset, last_ui_match, code_blocks

        time.sleep(0.15)

    if last_ui_match is not None:
        return current_offset, last_ui_match, code_blocks
    raise TimeoutError(f"No ui_auto_debug trace observed for session {session_id}")


def wait_for_trace_bundle_by_session_name(
    log_path: pathlib.Path,
    offset: int,
    *,
    session_name: str,
    timeout_s: float,
    quiet_s: float = 0.5,
) -> tuple[int, dict[str, str], list[dict[str, str]]]:
    deadline = time.time() + timeout_s
    quiet_deadline: float | None = None
    current_offset = offset
    last_ui_match: dict[str, str] | None = None
    code_blocks: list[dict[str, str]] = []

    while time.time() < deadline:
        current_offset, lines = read_new_lines(log_path, current_offset)
        saw_new_relevant_line = False
        for line in lines:
            if "ui_auto_debug_code_block" in line:
                code_blocks.append(parse_trace_fields(line))
                saw_new_relevant_line = True
                continue
            if "ui_auto_debug" not in line:
                continue
            fields = parse_trace_fields(line)
            if fields.get("session_name") != session_name:
                continue
            last_ui_match = fields
            saw_new_relevant_line = True

        if last_ui_match is not None:
            if saw_new_relevant_line:
                quiet_deadline = time.time() + quiet_s
            elif quiet_deadline is not None and time.time() >= quiet_deadline:
                return current_offset, last_ui_match, code_blocks

        time.sleep(0.15)

    if last_ui_match is not None:
        return current_offset, last_ui_match, code_blocks
    raise TimeoutError(f"No ui_auto_debug trace observed for session name {session_name!r}")


def capture_window_screenshot(window_id: str, path: pathlib.Path) -> None:
    run(["import", "-window", window_id, str(path)])


def crop_metric(image_path: pathlib.Path, x: int, y: int, width: int, height: int) -> dict[str, float]:
    geometry = f"{width}x{height}+{x}+{y}"
    stddev = run(
        [
            "convert",
            str(image_path),
            "-crop",
            geometry,
            "+repage",
            "-colorspace",
            "Gray",
            "-format",
            "%[fx:standard_deviation]",
            "info:",
        ]
    ).stdout.strip()
    colors = run(
        [
            "identify",
            "-format",
            "%k",
            str(image_path) + f"[{geometry}]",
        ]
    ).stdout.strip()
    return {
        "stddev": float(stddev or 0.0),
        "colors": float(colors or 0.0),
    }


def image_size(image_path: pathlib.Path) -> tuple[int, int]:
    output = run(["identify", "-format", "%w %h", str(image_path)]).stdout.strip()
    width_str, height_str = output.split()
    return int(width_str), int(height_str)


def heuristic_text_visible(metric: dict[str, float]) -> bool:
    return metric["stddev"] >= 0.01 or metric["colors"] >= 16.0


def parse_widths(raw: str) -> list[int]:
    return [int(part.strip()) for part in raw.split(",") if part.strip()]


def build_output_dir(output_dir: str | None) -> pathlib.Path:
    if output_dir:
        path = pathlib.Path(output_dir)
    else:
        stamp = dt.datetime.now().strftime("%Y%m%d-%H%M%S")
        path = ROOT / "tmp" / f"auto-ui-debug-{stamp}"
    path.mkdir(parents=True, exist_ok=True)
    return path

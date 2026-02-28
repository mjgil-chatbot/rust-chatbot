#!/usr/bin/env python3
from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import pathlib
import re
import sys
from dataclasses import dataclass
from typing import Any


TRANSCRIPT_STATUS_PREFIXES = ("● [", "○ [", "◦ [", "• [")
STANDARD_TRANSCRIPT_LINE_RE = re.compile(
    r"^\[[A-Z][a-z]{2} \d{1,2} \d{2}:\d{2}[ap]m\] .+"
)
STANDARD_TOOL_TRANSCRIPT_LINE_RE = re.compile(
    r"^\[[A-Z][a-z]{2} \d{1,2} \d{2}:\d{2}[ap]m\] [^|\n]+ \| .+"
)


@dataclass
class TranscriptSection:
    provider_dir: str
    session_id: str
    session_name: str
    session_updated_at: str
    message_index: int
    message_timestamp: str
    transcript_line_count: int
    tool_line_count: int
    has_markdown_body: bool
    transcript: str
    markdown_body: str


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description=(
            "Extract the assistant transcript/tool sections that rust-chatbot "
            "persisted for recent UI-visible sessions."
        )
    )
    parser.add_argument(
        "--days",
        type=float,
        default=2.0,
        help="Only include assistant messages from the last N days. Default: 2.",
    )
    parser.add_argument(
        "--root",
        type=pathlib.Path,
        default=default_runtime_root(),
        help="Shared runtime root to scan. Default: ~/.rust-chatbot or the configured override.",
    )
    parser.add_argument(
        "--provider-dir",
        action="append",
        default=[],
        help="Limit scanning to one or more runtime subdirectories like .codex-desktop.",
    )
    parser.add_argument(
        "--include-hidden",
        action="store_true",
        help="Include sessions marked hidden in sessions.json.",
    )
    parser.add_argument(
        "--include-transcript-only",
        action="store_true",
        help="Include transcript blocks that do not have a markdown answer body after them.",
    )
    parser.add_argument(
        "--format",
        choices=("markdown", "json"),
        default="markdown",
        help="Output format. Default: markdown.",
    )
    parser.add_argument(
        "--output",
        type=pathlib.Path,
        help="Write the extracted report to this path instead of stdout.",
    )
    return parser.parse_args()


def default_runtime_root() -> pathlib.Path:
    override = os.environ.get("RUST_CHATBOT_SHARED_ROOT") or os.environ.get(
        "RUST_CHATBOT_CONFIG_HOME"
    )
    if override:
        return pathlib.Path(override).expanduser()
    return pathlib.Path.home() / ".rust-chatbot"


def parse_timestamp(raw: str) -> dt.datetime | None:
    if not raw:
        return None
    normalized = raw.strip()
    if normalized.endswith("Z"):
        normalized = normalized[:-1] + "+00:00"
    try:
        parsed = dt.datetime.fromisoformat(normalized)
    except ValueError:
        return None
    if parsed.tzinfo is None:
        return parsed.replace(tzinfo=dt.timezone.utc)
    return parsed.astimezone(dt.timezone.utc)


def discover_provider_dirs(root: pathlib.Path, requested: list[str]) -> list[pathlib.Path]:
    if requested:
        candidates = [root / item for item in requested]
    else:
        candidates = [child for child in sorted(root.iterdir()) if child.is_dir()]

    provider_dirs: list[pathlib.Path] = []
    for candidate in candidates:
        if not candidate.exists() or not candidate.is_dir():
            continue
        if (candidate / "sessions.json").is_file() and (candidate / "sessions").is_dir():
            provider_dirs.append(candidate)
    return provider_dirs


def load_hidden_map(provider_dir: pathlib.Path) -> dict[str, bool]:
    sessions_json = provider_dir / "sessions.json"
    try:
        payload = json.loads(sessions_json.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    raw_sessions = payload.get("sessions", {})
    if not isinstance(raw_sessions, dict):
        return {}

    hidden: dict[str, bool] = {}
    for session_id, raw in raw_sessions.items():
        if isinstance(raw, dict):
            hidden[session_id] = bool(raw.get("hidden"))
    return hidden


def is_transcript_line(line: str) -> bool:
    stripped = line.strip()
    return bool(stripped) and (
        stripped.startswith(TRANSCRIPT_STATUS_PREFIXES)
        or STANDARD_TRANSCRIPT_LINE_RE.match(stripped) is not None
    )


def is_tool_line(line: str) -> bool:
    stripped = line.strip()
    return stripped.startswith(TRANSCRIPT_STATUS_PREFIXES) or (
        STANDARD_TOOL_TRANSCRIPT_LINE_RE.match(stripped) is not None
    )


def split_leading_transcript(content: str) -> tuple[str, str] | None:
    lines = content.splitlines()
    transcript_lines: list[str] = []
    index = 0
    while index < len(lines):
        line = lines[index]
        if not line.strip():
            break
        if not is_transcript_line(line):
            break
        transcript_lines.append(line)
        index += 1

    if not transcript_lines:
        return None

    markdown_body = "\n".join(lines[index:]).lstrip("\r\n")
    transcript = "\n".join(transcript_lines)
    return transcript, markdown_body


def load_sections(
    provider_dir: pathlib.Path,
    *,
    cutoff: dt.datetime,
    include_hidden: bool,
    include_transcript_only: bool,
) -> list[TranscriptSection]:
    hidden_map = load_hidden_map(provider_dir)
    sections: list[TranscriptSection] = []

    for session_path in sorted((provider_dir / "sessions").glob("*.json")):
        try:
            payload = json.loads(session_path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError):
            continue

        session_id = str(payload.get("id") or session_path.stem)
        if not include_hidden and hidden_map.get(session_id, False):
            continue

        session_name = str(payload.get("name") or session_id)
        session_updated_at = str(payload.get("updated_at") or "")
        messages = payload.get("messages", [])
        if not isinstance(messages, list):
            continue

        for message_index, message in enumerate(messages):
            if not isinstance(message, dict) or bool(message.get("is_user")):
                continue

            message_timestamp = str(message.get("timestamp") or "")
            parsed_message_time = parse_timestamp(message_timestamp)
            if parsed_message_time is None or parsed_message_time < cutoff:
                continue

            content = message.get("content")
            if not isinstance(content, str) or not content.strip():
                continue

            split = split_leading_transcript(content)
            if split is None:
                continue
            transcript, markdown_body = split
            transcript_lines = transcript.splitlines()
            tool_line_count = sum(1 for line in transcript_lines if is_tool_line(line))
            if tool_line_count == 0:
                continue
            has_markdown_body = bool(markdown_body.strip())
            if not include_transcript_only and not has_markdown_body:
                continue

            sections.append(
                TranscriptSection(
                    provider_dir=provider_dir.name,
                    session_id=session_id,
                    session_name=session_name,
                    session_updated_at=session_updated_at,
                    message_index=message_index,
                    message_timestamp=message_timestamp,
                    transcript_line_count=len(transcript_lines),
                    tool_line_count=tool_line_count,
                    has_markdown_body=has_markdown_body,
                    transcript=transcript,
                    markdown_body=markdown_body,
                )
            )

    sections.sort(
        key=lambda item: (
            parse_timestamp(item.message_timestamp) or dt.datetime.min.replace(tzinfo=dt.timezone.utc),
            item.provider_dir,
            item.session_id,
            item.message_index,
        )
    )
    return sections


def render_markdown(
    sections: list[TranscriptSection],
    *,
    root: pathlib.Path,
    cutoff: dt.datetime,
) -> str:
    lines = [
        "# UI Tool Output Sections",
        "",
        f"- Runtime root: `{root}`",
        f"- Cutoff (UTC): `{cutoff.isoformat()}`",
        f"- Sections: `{len(sections)}`",
        "",
    ]

    if not sections:
        lines.append("No matching tool-output transcript sections were found.")
        lines.append("")
        return "\n".join(lines)

    for index, section in enumerate(sections, start=1):
        lines.extend(
            [
                (
                    f"## {index}. {section.provider_dir} / {section.session_name} "
                    f"({section.session_id})"
                ),
                "",
                f"- Session updated: `{section.session_updated_at}`",
                f"- Assistant message index: `{section.message_index}`",
                f"- Assistant message timestamp: `{section.message_timestamp}`",
                f"- Transcript lines: `{section.transcript_line_count}`",
                f"- Tool-style lines: `{section.tool_line_count}`",
                f"- Has markdown body: `{str(section.has_markdown_body).lower()}`",
                "",
                "### Transcript",
                "",
                "```markdown",
                section.transcript,
                "```",
                "",
            ]
        )
        if section.has_markdown_body:
            lines.extend(
                [
                    "### Markdown Body",
                    "",
                    "```markdown",
                    section.markdown_body,
                    "```",
                    "",
                ]
            )

    return "\n".join(lines)


def render_json(sections: list[TranscriptSection], *, root: pathlib.Path, cutoff: dt.datetime) -> str:
    payload: dict[str, Any] = {
        "runtime_root": str(root),
        "cutoff_utc": cutoff.isoformat(),
        "section_count": len(sections),
        "sections": [
            {
                "provider_dir": section.provider_dir,
                "session_id": section.session_id,
                "session_name": section.session_name,
                "session_updated_at": section.session_updated_at,
                "message_index": section.message_index,
                "message_timestamp": section.message_timestamp,
                "transcript_line_count": section.transcript_line_count,
                "tool_line_count": section.tool_line_count,
                "has_markdown_body": section.has_markdown_body,
                "transcript": section.transcript,
                "markdown_body": section.markdown_body,
            }
            for section in sections
        ],
    }
    return json.dumps(payload, indent=2, ensure_ascii=False) + "\n"


def write_output(text: str, output: pathlib.Path | None) -> None:
    if output is None:
        sys.stdout.write(text)
        return
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(text, encoding="utf-8")


def main() -> int:
    args = parse_args()
    root = args.root.expanduser()
    if not root.exists():
        raise SystemExit(f"Runtime root does not exist: {root}")

    cutoff = dt.datetime.now(dt.timezone.utc) - dt.timedelta(days=args.days)
    provider_dirs = discover_provider_dirs(root, args.provider_dir)
    if not provider_dirs:
        raise SystemExit(f"No session stores were found under {root}")

    sections: list[TranscriptSection] = []
    for provider_dir in provider_dirs:
        sections.extend(
            load_sections(
                provider_dir,
                cutoff=cutoff,
                include_hidden=args.include_hidden,
                include_transcript_only=args.include_transcript_only,
            )
        )

    if args.format == "json":
        rendered = render_json(sections, root=root, cutoff=cutoff)
    else:
        rendered = render_markdown(sections, root=root, cutoff=cutoff)
    write_output(rendered, args.output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

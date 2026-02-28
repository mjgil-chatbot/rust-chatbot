#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parent.parent
REGISTRY_DIR = ROOT / "workflow-registry" / "commands"
CLAUDE_DIR = ROOT / ".claude" / "commands"
CODEX_DIR = ROOT / ".codex" / "skills"
COMMAND_LIST = ROOT / "command-list.txt"


def home_dirs() -> tuple[Path, Path]:
    home = Path.home()
    return home / ".claude" / "commands", home / ".codex" / "skills"


def load_specs() -> list[dict[str, Any]]:
    specs: list[dict[str, Any]] = []
    for path in sorted(REGISTRY_DIR.glob("*.json")):
        with path.open("r", encoding="utf-8") as fh:
            spec = json.load(fh)
        spec["_source"] = path
        specs.append(spec)
    specs.sort(key=lambda item: (item.get("section", ""), item.get("order", 0), item["name"]))
    return specs


def bullets(items: list[str], empty: str = "- None") -> str:
    if not items:
        return empty
    return "\n".join(f"- {item}" for item in items)


def render_inputs(spec: dict[str, Any]) -> str:
    inputs = spec.get("inputs", [])
    if not inputs:
        return "- No structured inputs beyond the command arguments."
    lines = []
    for item in inputs:
        required = "required" if item.get("required") else "optional"
        lines.append(f"- `{item['name']}` ({required}): {item['description']}")
    return "\n".join(lines)


def render_claude_command(spec: dict[str, Any]) -> str:
    args = spec.get("arguments", "").strip()
    args_line = f"/{spec['name']} {args}".rstrip()
    examples = spec.get("examples", [])
    examples_block = "\n".join(f"```text\n{example}\n```" for example in examples)
    return f"""# {spec['title']}

Use this command to {spec['summary'][0].lower() + spec['summary'][1:]}

Invocation:

```text
{args_line}
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

{bullets(spec.get("when_to_use", []))}

## Inputs

{render_inputs(spec)}

## Context to load

{bullets(spec.get("context_files", []))}

## Workflow

{chr(10).join(f"{idx}. {step}" for idx, step in enumerate(spec.get("steps", []), start=1))}

## Expected outputs

{bullets(spec.get("outputs", []))}

## Examples

{examples_block or "```text\n/" + spec["name"] + "\n```"}
"""


def render_codex_skill(spec: dict[str, Any]) -> str:
    args = spec.get("arguments", "").strip()
    args_line = f"/{spec['name']} {args}".rstrip()
    examples = spec.get("examples", [])
    examples_block = "\n".join(f"```text\n{example}\n```" for example in examples)
    description = (
        "Use when the user wants to "
        + spec["summary"][0].lower()
        + spec["summary"][1:]
    )
    return f"""---
name: {json.dumps(spec["name"])}
description: {json.dumps(description)}
---

# {spec['title']}

Use this skill when the user wants to {spec['summary'][0].lower() + spec['summary'][1:]}

## When to use

{bullets(spec.get("when_to_use", []))}

## Inputs

{render_inputs(spec)}

## Required context

{bullets(spec.get("context_files", []))}

## Steps

{chr(10).join(f"{idx}. {step}" for idx, step in enumerate(spec.get("steps", []), start=1))}

## Output contract

{bullets(spec.get("outputs", []))}

## Example invocation

{examples_block or "```text\n" + args_line + "\n```"}
"""


def render_command_list(specs: list[dict[str, Any]]) -> str:
    sections: dict[str, list[dict[str, Any]]] = {}
    for spec in specs:
        sections.setdefault(spec.get("section", "general"), []).append(spec)

    preferred_order = ("general", "coordination", "memory")
    ordered_sections = [
        section_name for section_name in preferred_order if section_name in sections
    ]
    ordered_sections.extend(
        sorted(
            section_name
            for section_name in sections
            if section_name not in preferred_order
        )
    )

    lines: list[str] = []
    for section_name in ordered_sections:
        section_specs = sections.get(section_name, [])
        if not section_specs:
            continue
        for spec in section_specs:
            args = spec.get("arguments", "").strip()
            line = f"/{spec['name']} {args}".rstrip()
            lines.append(line)
        lines.append("")
    return "\n".join(lines).rstrip() + "\n"


def write_file(path: Path, content: str, check: bool) -> bool:
    existing = path.read_text(encoding="utf-8") if path.exists() else None
    if existing == content:
        return False
    if check:
        raise SystemExit(f"Generated file is out of date: {path}")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content, encoding="utf-8")
    return True


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate Claude and Codex workflow files.")
    parser.add_argument("--check", action="store_true", help="Fail if generated files are out of date.")
    parser.add_argument(
        "--install-home-memory",
        action="store_true",
        help="Also publish memory workflow wrappers into ~/.claude/commands and ~/.codex/skills.",
    )
    args = parser.parse_args()

    specs = load_specs()
    changed = 0

    CLAUDE_DIR.mkdir(parents=True, exist_ok=True)
    CODEX_DIR.mkdir(parents=True, exist_ok=True)

    for spec in specs:
        claude_path = CLAUDE_DIR / f"{spec['name']}.md"
        codex_path = CODEX_DIR / spec["name"] / "SKILL.md"
        if write_file(claude_path, render_claude_command(spec), args.check):
            changed += 1
        if write_file(codex_path, render_codex_skill(spec), args.check):
            changed += 1

    if args.install_home_memory:
        home_claude_dir, home_codex_dir = home_dirs()
        home_claude_dir.mkdir(parents=True, exist_ok=True)
        home_codex_dir.mkdir(parents=True, exist_ok=True)

        for spec in specs:
            if spec.get("section") != "memory":
                continue
            home_claude_path = home_claude_dir / f"{spec['name']}.md"
            home_codex_path = home_codex_dir / spec["name"] / "SKILL.md"
            if write_file(home_claude_path, render_claude_command(spec), args.check):
                changed += 1
            if write_file(home_codex_path, render_codex_skill(spec), args.check):
                changed += 1

    if write_file(COMMAND_LIST, render_command_list(specs), args.check):
        changed += 1

    if not args.check:
        print(f"Updated {changed} generated file(s).")


if __name__ == "__main__":
    main()

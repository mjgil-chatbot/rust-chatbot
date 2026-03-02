#!/usr/bin/env python3
import argparse
import fnmatch
import os
import re
import subprocess
import sys
from pathlib import Path


ALLOW_START = "// ui-thread-guard: allow-blocking-start"
ALLOW_END = "// ui-thread-guard: allow-blocking-end"
PROTECTED_GLOBS = (
    "src/app/ui/*.rs",
    "src/app/ui/**/*.rs",
    "src/app/state/messaging.rs",
    "src/app/state/persistence.rs",
)
EXCLUDED_GLOBS = (
    "src/app/ui/*_tests.rs",
    "src/app/ui/**/_tests.rs",
    "src/app/ui/**/tests.rs",
    "src/app/ui/**/tests/**/*.rs",
)
DISALLOWED_PATTERNS = (
    ("filesystem IO", re.compile(r"\bstd::fs::")),
    ("process spawn", re.compile(r"\b(?:std::process::Command|Command::new)\b")),
    ("thread sleep", re.compile(r"\b(?:std::thread::sleep|thread::sleep)\s*\(")),
    ("blocking wait", re.compile(r"\.wait\s*\(")),
    ("blocking receive", re.compile(r"\.(?:recv|recv_timeout)\s*\(")),
    ("filesystem metadata", re.compile(r"\.(?:canonicalize|exists|metadata)\s*\(")),
    ("session storage access", re.compile(r"\bDataManager::new\s*\(")),
    ("session height-cache access", re.compile(r"\.load_height_cache\s*\(")),
    ("sqlite access", re.compile(r"\brusqlite::")),
    ("network client", re.compile(r"\breqwest::")),
)
AUTOMATION_CERT_REL = Path("generated/rust/AUTOMATION_CERTIFICATE.txt")
EXECUTION_CONTRACT_REL = Path("generated/rust/EXECUTION_CONTRACT.md")
README_REL = Path("generated/rust/README.md")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the ui-thread contract against ui-lean artifacts and repo-local guards."
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=None,
        help="Repository root. Defaults to git rev-parse --show-toplevel.",
    )
    parser.add_argument(
        "--ui-lean-root",
        type=Path,
        default=None,
        help=(
            "Path to the ui-lean checkout. Defaults to $RUST_CHATBOT_UI_LEAN_ROOT, "
            "then repo-adjacent ui-lean candidates, then ~/git/ui-lean."
        ),
    )
    return parser.parse_args()


def resolve_repo_root(explicit: Path | None) -> Path:
    if explicit is not None:
        return explicit.resolve()
    script_repo_root = Path(__file__).resolve().parent.parent
    result = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        check=False,
        cwd=script_repo_root,
        capture_output=True,
        text=True,
    )
    if result.returncode == 0:
        return Path(result.stdout.strip()).resolve()
    return script_repo_root


def resolve_ui_lean_root(explicit: Path | None, repo_root: Path) -> Path:
    if explicit is not None:
        return explicit.expanduser().resolve()
    configured = os.environ.get("RUST_CHATBOT_UI_LEAN_ROOT")
    if configured:
        return Path(configured).expanduser().resolve()

    candidates: list[Path] = []
    seen: set[Path] = set()
    for candidate in (
        repo_root.parent / "ui-lean",
        repo_root.parent.parent / "ui-lean",
        Path.home() / "git" / "ui-lean",
    ):
        resolved = candidate.expanduser().resolve()
        if resolved in seen:
            continue
        seen.add(resolved)
        candidates.append(resolved)

    for candidate in candidates:
        if candidate.is_dir():
            return candidate

    return candidates[0]


def is_excluded(relative: str) -> bool:
    return (
        "/tests/" in relative
        or relative.endswith("_tests.rs")
        or relative.endswith("/tests.rs")
        or any(fnmatch.fnmatch(relative, pattern) for pattern in EXCLUDED_GLOBS)
    )


def collect_protected_files(repo_root: Path) -> list[Path]:
    files: set[Path] = set()
    for pattern in PROTECTED_GLOBS:
        files.update(path for path in repo_root.glob(pattern) if path.is_file())
    filtered = []
    for path in sorted(files):
        relative = path.relative_to(repo_root).as_posix()
        if is_excluded(relative):
            continue
        filtered.append(path)
    return filtered


def verify_ui_lean_artifacts(ui_lean_root: Path) -> list[str]:
    failures: list[str] = []
    automation_cert = ui_lean_root / AUTOMATION_CERT_REL
    execution_contract = ui_lean_root / EXECUTION_CONTRACT_REL
    readme = ui_lean_root / README_REL

    required_files = (automation_cert, execution_contract, readme)
    for path in required_files:
        if not path.is_file():
            failures.append(f"missing ui-lean artifact: {path}")

    if failures:
        return failures

    automation_text = automation_cert.read_text(encoding="utf-8")
    execution_text = execution_contract.read_text(encoding="utf-8")
    readme_text = readme.read_text(encoding="utf-8")

    required_checks = (
        ("automation certificate is proved", "status: proved" in automation_text),
        (
            "automation certificate proves no blocking work on the UI thread",
            "no_blocking_work_on_ui_thread: proven" in automation_text,
        ),
        (
            "automation certificate proves bounded UI-thread work",
            "bounded_ui_thread_work: proven" in automation_text,
        ),
        (
            "execution contract states generated handlers avoid blocking work",
            "Generated UI handlers are certified to avoid blocking work on the UI thread."
            in execution_text,
        ),
        (
            "generated README records the nonblocking UI-thread dispatch guarantee",
            "generated UI dispatch performs no blocking/CPU/wait/custom uncertified work inline on the UI thread"
            in readme_text,
        ),
    )
    for description, passed in required_checks:
        if not passed:
            failures.append(f"ui-lean artifact check failed: {description}")
    return failures


def scan_file_for_violations(repo_root: Path, path: Path) -> list[str]:
    failures: list[str] = []
    lines = path.read_text(encoding="utf-8").splitlines()
    in_allow_region = False

    for index, line in enumerate(lines, start=1):
        if ALLOW_START in line:
            if in_allow_region:
                failures.append(
                    f"{path.relative_to(repo_root).as_posix()}:{index}: nested allow-blocking region"
                )
            in_allow_region = True
            continue
        if ALLOW_END in line:
            if not in_allow_region:
                failures.append(
                    f"{path.relative_to(repo_root).as_posix()}:{index}: unmatched allow-blocking end marker"
                )
            in_allow_region = False
            continue
        if in_allow_region:
            continue
        for description, pattern in DISALLOWED_PATTERNS:
            if pattern.search(line):
                failures.append(
                    f"{path.relative_to(repo_root).as_posix()}:{index}: inline {description}: {line.strip()}"
                )

    if in_allow_region:
        failures.append(
            f"{path.relative_to(repo_root).as_posix()}: missing allow-blocking end marker"
        )
    return failures


def main() -> int:
    args = parse_args()
    repo_root = resolve_repo_root(args.repo_root)
    ui_lean_root = resolve_ui_lean_root(args.ui_lean_root, repo_root)
    protected_files = collect_protected_files(repo_root)

    failures = verify_ui_lean_artifacts(ui_lean_root)
    for path in protected_files:
        failures.extend(scan_file_for_violations(repo_root, path))

    if failures:
        for failure in failures:
            print(f"ui-thread-contract: FAIL: {failure}", file=sys.stderr)
        return 1

    print(
        f"ui-thread-contract: PASS: verified {len(protected_files)} protected file(s) "
        f"against ui-lean artifacts from {ui_lean_root}"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

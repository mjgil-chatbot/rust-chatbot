#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys
from pathlib import Path


LIFECYCLE_DOC_REL = Path("docs/architecture/session-worker-lifecycle-spec.md")
VERIFY_DOC_REL = Path("docs/verification.md")
REGRESSION_SCRIPT_REL = Path("scripts/verify-session-lifecycle-regressions.sh")
REQUIRED_DOC_SNIPPETS = {
    LIFECYCLE_DOC_REL: (
        "### 2. `PendingDraft`",
        "### 6. `TerminalPendingReconcile`",
        "### 8. `PrunedStartupGap`",
        "### 9. `RecoveredError` / `RecoveredStopped`",
        "scripts/verify_session_lifecycle_contract.py",
        "scripts/verify-session-lifecycle-regressions.sh",
    ),
    VERIFY_DOC_REL: (
        "## Session/Worker Lifecycle Contract",
        "scripts/verify_session_lifecycle_contract.py",
        "scripts/verify-session-lifecycle-regressions.sh",
        "session/worker lifecycle contract",
    ),
}
REQUIRED_TESTS = {
    Path("src/app/state/ui_spec_lifecycle_tests.rs"): (
        "submit_valid_creates_pending_draft_and_requests_focus",
    ),
    Path("src/app/state/session_runtime_tests.rs"): (
        "apply_stream_to_current_clears_pending_draft_on_first_chunk",
        "local_terminal_worker_clears_pending_draft",
        "local_terminal_worker_clears_pending_draft_even_on_empty_response",
        "remote_terminal_update_clears_pending_draft",
    ),
    Path("src/app/state/session_runtime_terminal_persistence_tests.rs"): (
        "local_terminal_worker_persists_session_before_worker_cleanup",
    ),
    Path("src/app/state/sync_slots_tests.rs"): (
        "poll_worker_slots_clears_processing_for_stale_request_only_spool_entry",
    ),
    Path("src/app/state/ui_io_tests.rs"): (
        "selected_session_reload_skips_replacement_when_pending_draft_active",
    ),
    Path("src/app/state/ui_io_reload_guard_tests.rs"): (
        "selected_session_reload_drops_divergent_newer_snapshot_for_current_chat",
        "selected_session_reload_accepts_strict_extension_even_with_equal_timestamp",
    ),
}
REQUIRED_REGRESSION_TESTS = (
    "submit_valid_creates_pending_draft_and_requests_focus",
    "apply_stream_to_current_clears_pending_draft_on_first_chunk",
    "local_terminal_worker_clears_pending_draft",
    "local_terminal_worker_clears_pending_draft_even_on_empty_response",
    "remote_terminal_update_clears_pending_draft",
    "local_terminal_worker_persists_session_before_worker_cleanup",
    "poll_worker_slots_clears_processing_for_stale_request_only_spool_entry",
    "selected_session_reload_skips_replacement_when_pending_draft_active",
    "selected_session_reload_drops_divergent_newer_snapshot_for_current_chat",
    "selected_session_reload_accepts_strict_extension_even_with_equal_timestamp",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the repo-local session/worker lifecycle contract."
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=None,
        help="Repository root. Defaults to git rev-parse --show-toplevel.",
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


def read_text(repo_root: Path, relative: Path) -> str:
    return (repo_root / relative).read_text(encoding="utf-8")


def verify_docs(repo_root: Path) -> list[str]:
    failures: list[str] = []
    for relative, snippets in REQUIRED_DOC_SNIPPETS.items():
        path = repo_root / relative
        if not path.is_file():
            failures.append(f"missing documentation artifact: {relative.as_posix()}")
            continue
        content = read_text(repo_root, relative)
        for snippet in snippets:
            if snippet not in content:
                failures.append(
                    f"{relative.as_posix()}: missing required lifecycle snippet: {snippet}"
                )
    return failures


def verify_tests(repo_root: Path) -> list[str]:
    failures: list[str] = []
    for relative, test_names in REQUIRED_TESTS.items():
        path = repo_root / relative
        if not path.is_file():
            failures.append(f"missing lifecycle regression source file: {relative.as_posix()}")
            continue
        content = read_text(repo_root, relative)
        for test_name in test_names:
            if re.search(rf"\bfn\s+{re.escape(test_name)}\s*\(", content) is None:
                failures.append(
                    f"{relative.as_posix()}: missing required lifecycle regression `{test_name}`"
                )
    return failures


def verify_regression_script(repo_root: Path) -> list[str]:
    failures: list[str] = []
    path = repo_root / REGRESSION_SCRIPT_REL
    if not path.is_file():
        return [f"missing lifecycle regression script: {REGRESSION_SCRIPT_REL.as_posix()}"]

    content = path.read_text(encoding="utf-8")
    for test_name in REQUIRED_REGRESSION_TESTS:
        if test_name not in content:
            failures.append(
                f"{REGRESSION_SCRIPT_REL.as_posix()}: missing focused lifecycle regression `{test_name}`"
            )
    return failures


def main() -> int:
    args = parse_args()
    repo_root = resolve_repo_root(args.repo_root)

    failures = [
        *verify_docs(repo_root),
        *verify_tests(repo_root),
        *verify_regression_script(repo_root),
    ]

    if failures:
        for failure in failures:
            print(f"session-lifecycle-contract: FAIL: {failure}", file=sys.stderr)
        return 1

    print(
        "session-lifecycle-contract: PASS: verified lifecycle docs, focused regressions, and contract wiring"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

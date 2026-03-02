#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys
from pathlib import Path


HISTORY_DOC_REL = Path("docs/architecture/chatbot-history-integration.md")
VERIFY_DOC_REL = Path("docs/verification.md")
REGRESSION_SCRIPT_REL = Path("scripts/verify-history-regressions.sh")
REQUIRED_DOC_SNIPPETS = {
    HISTORY_DOC_REL: (
        "byte-for-byte",
        "missing per-turn checkpoints",
        "Current Native Enforcement",
        "scripts/verify_history_contract.py",
        "scripts/verify-history-regressions.sh",
    ),
    VERIFY_DOC_REL: (
        "## History/Checkpoint Contract",
        "scripts/verify_history_contract.py",
        "scripts/verify-history-regressions.sh",
        "history/checkpoint contract",
    ),
}
REQUIRED_TESTS = {
    Path("src/app/state/history_tests.rs"): (
        "poll_history_sync_loads_missing_store_notice_via_background_refresh",
        "drain_history_results_clears_stuck_refresh_on_disconnect",
        "drain_history_results_clears_stuck_checkpoint_on_disconnect",
        "load_history_snapshot_restores_deleted_store_byte_for_byte",
        "build_checkpoint_manifest_skips_invalid_cargo_manifest_for_companion_discovery",
        "build_checkpoint_manifest_ignores_unrelated_dirty_sibling_repo_patches",
        "sync_chat_history_backfills_missing_auto_turn_checkpoints",
        "load_history_snapshot_exposes_checkpoint_repo_summaries",
    ),
    Path("src/app/state/history_repo_capture_tests.rs"): (
        "build_checkpoint_manifest_omits_oversized_untracked_repo_patch",
        "build_checkpoint_manifest_omits_repo_patch_when_remaining_quota_is_too_small",
    ),
}
REQUIRED_REGRESSION_TESTS = (
    "poll_history_sync_loads_missing_store_notice_via_background_refresh",
    "drain_history_results_clears_stuck_refresh_on_disconnect",
    "drain_history_results_clears_stuck_checkpoint_on_disconnect",
    "load_history_snapshot_restores_deleted_store_byte_for_byte",
    "build_checkpoint_manifest_skips_invalid_cargo_manifest_for_companion_discovery",
    "build_checkpoint_manifest_ignores_unrelated_dirty_sibling_repo_patches",
    "build_checkpoint_manifest_omits_oversized_untracked_repo_patch",
    "build_checkpoint_manifest_omits_repo_patch_when_remaining_quota_is_too_small",
    "sync_chat_history_backfills_missing_auto_turn_checkpoints",
    "load_history_snapshot_exposes_checkpoint_repo_summaries",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the repo-local history/checkpoint contract."
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
                    f"{relative.as_posix()}: missing required history snippet: {snippet}"
                )
    return failures


def verify_tests(repo_root: Path) -> list[str]:
    failures: list[str] = []
    for relative, test_names in REQUIRED_TESTS.items():
        path = repo_root / relative
        if not path.is_file():
            failures.append(f"missing history regression source file: {relative.as_posix()}")
            continue
        content = read_text(repo_root, relative)
        for test_name in test_names:
            if re.search(rf"\bfn\s+{re.escape(test_name)}\s*\(", content) is None:
                failures.append(
                    f"{relative.as_posix()}: missing required history regression `{test_name}`"
                )
    return failures


def verify_regression_script(repo_root: Path) -> list[str]:
    failures: list[str] = []
    path = repo_root / REGRESSION_SCRIPT_REL
    if not path.is_file():
        return [f"missing history regression script: {REGRESSION_SCRIPT_REL.as_posix()}"]

    content = path.read_text(encoding="utf-8")
    for test_name in REQUIRED_REGRESSION_TESTS:
        if test_name not in content:
            failures.append(
                f"{REGRESSION_SCRIPT_REL.as_posix()}: missing focused history regression `{test_name}`"
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
            print(f"history-contract: FAIL: {failure}", file=sys.stderr)
        return 1

    print(
        "history-contract: PASS: verified history/checkpoint docs, focused regressions, and contract wiring"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

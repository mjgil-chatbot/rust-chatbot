#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys
from pathlib import Path


VERSIONING_DOC_REL = Path("docs/architecture/spool-session-contract-versioning.md")
WORKER_SPOOL_DOC_REL = Path("docs/architecture/worker-spool.md")
VERIFY_DOC_REL = Path("docs/verification.md")
REGRESSION_SCRIPT_REL = Path("scripts/verify-spool-session-regressions.sh")
REQUIRED_DOC_SNIPPETS = {
    VERSIONING_DOC_REL: (
        "spool/session contract version = v1",
        "Compatible additive",
        "Behavioral compatible tightening",
        "Breaking contract",
        "scripts/verify_spool_session_contract.py",
        "scripts/verify-spool-session-regressions.sh",
    ),
    WORKER_SPOOL_DOC_REL: (
        "schema_version = 1 on `request.json`",
        "schema_version = 1 on `state.json`",
        "schema_version = 1 on `result.json`",
        "Current Native Enforcement",
        "scripts/verify_spool_session_contract.py",
        "scripts/verify-spool-session-regressions.sh",
    ),
    VERIFY_DOC_REL: (
        "## Spool/Session Contract",
        "scripts/verify_spool_session_contract.py",
        "scripts/verify-spool-session-regressions.sh",
        "spool/session contract",
    ),
}
REQUIRED_TESTS = {
    Path("src/app/state/helpers_impl.rs"): (
        "read_worker_result_treats_unsupported_schema_version_as_malformed",
    ),
    Path("src/app/state/session_runtime_terminal_persistence_tests.rs"): (
        "local_terminal_worker_persists_session_before_worker_cleanup",
        "local_terminal_worker_keeps_existing_visible_assistant_when_terminal_response_is_think_only",
        "remote_terminal_update_reloads_newer_disk_session_before_applying_response",
    ),
    Path("src/app/state/sync_slots_tests.rs"): (
        "poll_worker_slots_clears_processing_for_stale_request_only_spool_entry",
        "poll_worker_slots_done_replaces_stale_provider_session_id",
    ),
    Path("src/session_mcp/service.rs"): (
        "inspect_session_surfaces_worker_spool_truncation_metadata",
    ),
}
REQUIRED_REGRESSION_TESTS = (
    "read_worker_result_treats_unsupported_schema_version_as_malformed",
    "local_terminal_worker_persists_session_before_worker_cleanup",
    "local_terminal_worker_keeps_existing_visible_assistant_when_terminal_response_is_think_only",
    "remote_terminal_update_reloads_newer_disk_session_before_applying_response",
    "poll_worker_slots_clears_processing_for_stale_request_only_spool_entry",
    "poll_worker_slots_done_replaces_stale_provider_session_id",
    "inspect_session_surfaces_worker_spool_truncation_metadata",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the repo-local spool/session contract."
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
                    f"{relative.as_posix()}: missing required spool/session snippet: {snippet}"
                )
    return failures


def verify_tests(repo_root: Path) -> list[str]:
    failures: list[str] = []
    for relative, test_names in REQUIRED_TESTS.items():
        path = repo_root / relative
        if not path.is_file():
            failures.append(f"missing spool/session regression source file: {relative.as_posix()}")
            continue
        content = read_text(repo_root, relative)
        for test_name in test_names:
            if re.search(rf"\bfn\s+{re.escape(test_name)}\s*\(", content) is None:
                failures.append(
                    f"{relative.as_posix()}: missing required spool/session regression `{test_name}`"
                )
    return failures


def verify_regression_script(repo_root: Path) -> list[str]:
    failures: list[str] = []
    path = repo_root / REGRESSION_SCRIPT_REL
    if not path.is_file():
        return [f"missing spool/session regression script: {REGRESSION_SCRIPT_REL.as_posix()}"]

    content = path.read_text(encoding="utf-8")
    for test_name in REQUIRED_REGRESSION_TESTS:
        if test_name not in content:
            failures.append(
                f"{REGRESSION_SCRIPT_REL.as_posix()}: missing focused spool/session regression `{test_name}`"
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
            print(f"spool-session-contract: FAIL: {failure}", file=sys.stderr)
        return 1

    print(
        "spool-session-contract: PASS: verified spool/session docs, focused regressions, and contract wiring"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

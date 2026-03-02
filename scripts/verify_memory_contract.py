#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys
from pathlib import Path


MEMORY_DOC_REL = Path("docs/architecture/memmachine-integration.md")
VERIFY_DOC_REL = Path("docs/verification.md")
REGRESSION_SCRIPT_REL = Path("scripts/verify-memory-regressions.sh")
REQUIRED_DOC_SNIPPETS = {
    MEMORY_DOC_REL: (
        "Retrieved memory",
        "/memory-retrievals",
        "invalidated_doc_keys",
        "Current Native Enforcement",
        "scripts/verify_memory_contract.py",
        "scripts/verify-memory-regressions.sh",
    ),
    VERIFY_DOC_REL: (
        "## Memory Contract",
        "scripts/verify_memory_contract.py",
        "scripts/verify-memory-regressions.sh",
        "memory contract",
    ),
}
REQUIRED_TESTS = {
    Path("src/memory_mcp/tests.rs"): (
        "workspace_memory_status_routes_successfully",
        "search_memory_routes_successfully",
        "list_memory_injections_returns_persisted_user_turn_previews",
        "list_memory_retrievals_returns_candidate_hits_and_injected_preview",
        "invalidate_doc_key_routes_successfully",
        "restore_doc_key_routes_successfully",
        "remember_fact_rejects_invalid_priority",
        "service_error_for_workspace_outside_allowed_roots",
    ),
}
REQUIRED_REGRESSION_TESTS = (
    "workspace_memory_status_routes_successfully",
    "search_memory_routes_successfully",
    "list_memory_injections_returns_persisted_user_turn_previews",
    "list_memory_retrievals_returns_candidate_hits_and_injected_preview",
    "invalidate_doc_key_routes_successfully",
    "restore_doc_key_routes_successfully",
    "remember_fact_rejects_invalid_priority",
    "service_error_for_workspace_outside_allowed_roots",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the repo-local memory contract."
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
                    f"{relative.as_posix()}: missing required memory snippet: {snippet}"
                )
    return failures


def verify_tests(repo_root: Path) -> list[str]:
    failures: list[str] = []
    for relative, test_names in REQUIRED_TESTS.items():
        path = repo_root / relative
        if not path.is_file():
            failures.append(f"missing memory regression source file: {relative.as_posix()}")
            continue
        content = read_text(repo_root, relative)
        for test_name in test_names:
            if re.search(rf"\bfn\s+{re.escape(test_name)}\s*\(", content) is None:
                failures.append(
                    f"{relative.as_posix()}: missing required memory regression `{test_name}`"
                )
    return failures


def verify_regression_script(repo_root: Path) -> list[str]:
    failures: list[str] = []
    path = repo_root / REGRESSION_SCRIPT_REL
    if not path.is_file():
        return [f"missing memory regression script: {REGRESSION_SCRIPT_REL.as_posix()}"]

    content = path.read_text(encoding="utf-8")
    for test_name in REQUIRED_REGRESSION_TESTS:
        if test_name not in content:
            failures.append(
                f"{REGRESSION_SCRIPT_REL.as_posix()}: missing focused memory regression `{test_name}`"
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
            print(f"memory-contract: FAIL: {failure}", file=sys.stderr)
        return 1

    print(
        "memory-contract: PASS: verified memory docs, focused regressions, and contract wiring"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

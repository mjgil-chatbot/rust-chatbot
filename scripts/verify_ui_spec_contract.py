#!/usr/bin/env python3
import argparse
import re
import subprocess
import sys
from pathlib import Path


SPEC_DOC_REL = Path("docs/architecture/ui-lean-send-draft-stream-spec.md")
INTEGRATION_DOC_REL = Path("docs/architecture/ui-lean-integration-map.md")
VERIFY_DOC_REL = Path("docs/verification.md")
REGRESSION_SCRIPT_REL = Path("scripts/verify-ui-spec-regressions.sh")
REQUIRED_DOC_SNIPPETS = {
    SPEC_DOC_REL: (
        "### Property 1: No Permanent Disabled-Composer State",
        "### Property 2: Real Assistant Output Replaces Synthetic Draft Authority",
        "### Property 3: Terminal Cleanup Clears Pending Draft Even With Empty Output",
        "### Property 4: Passive Refresh Cannot Override Authoritative Draft State",
        "### Property 5: Rendering Alone Does Not Mutate Draft Ownership",
        "scripts/verify_ui_spec_contract.py",
        "scripts/verify-ui-spec-regressions.sh",
    ),
    INTEGRATION_DOC_REL: (
        "scripts/verify_ui_spec_contract.py",
        "scripts/verify-ui-spec-regressions.sh",
        "submit_valid_creates_pending_draft_and_requests_focus",
    ),
    VERIFY_DOC_REL: (
        "scripts/verify_ui_spec_contract.py",
        "scripts/verify-ui-spec-regressions.sh",
        "send/draft/stream",
    ),
}
REQUIRED_TESTS = {
    Path("src/app/state/ui_spec_lifecycle_tests.rs"): (
        "submit_valid_creates_pending_draft_and_requests_focus",
        "local_terminal_empty_response_requests_focus_and_clears_pending_draft",
        "send_failed_returns_chat_to_recoverable_idle_state",
    ),
    Path("src/app/state/session_runtime_tests.rs"): (
        "apply_stream_to_current_clears_pending_draft_on_first_chunk",
        "apply_stream_to_current_empty_response_keeps_pending_draft",
        "local_terminal_worker_clears_pending_draft",
        "local_terminal_worker_clears_pending_draft_even_on_empty_response",
        "remote_terminal_update_clears_pending_draft",
        "remote_terminal_update_updates_current_session_owned_state",
        "switching_chats_preserves_pending_draft_for_original_chat",
    ),
    Path("src/app/state/ui_io_tests.rs"): (
        "selected_session_reload_skips_replacement_when_pending_draft_active",
        "selected_session_reload_proceeds_when_no_pending_draft",
        "selected_session_reload_proceeds_when_pending_draft_is_for_other_chat",
    ),
    Path("src/app/ui/message_list/tests/render_regressions.rs"): (
        "render_with_pending_draft_does_not_clear_or_panic",
        "render_with_pending_draft_for_other_chat_hides_indicator",
    ),
}
REQUIRED_REGRESSION_TESTS = (
    "submit_valid_creates_pending_draft_and_requests_focus",
    "apply_stream_to_current_clears_pending_draft_on_first_chunk",
    "apply_stream_to_current_empty_response_keeps_pending_draft",
    "local_terminal_worker_clears_pending_draft",
    "local_terminal_worker_clears_pending_draft_even_on_empty_response",
    "local_terminal_empty_response_requests_focus_and_clears_pending_draft",
    "remote_terminal_update_clears_pending_draft",
    "remote_terminal_update_updates_current_session_owned_state",
    "send_failed_returns_chat_to_recoverable_idle_state",
    "selected_session_reload_skips_replacement_when_pending_draft_active",
    "selected_session_reload_proceeds_when_no_pending_draft",
    "selected_session_reload_proceeds_when_pending_draft_is_for_other_chat",
    "switching_chats_preserves_pending_draft_for_original_chat",
    "render_with_pending_draft_does_not_clear_or_panic",
    "render_with_pending_draft_for_other_chat_hides_indicator",
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify the repo-local send/draft/stream ui-spec contract."
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
                    f"{relative.as_posix()}: missing required contract snippet: {snippet}"
                )
    return failures


def verify_tests(repo_root: Path) -> list[str]:
    failures: list[str] = []
    for relative, test_names in REQUIRED_TESTS.items():
        path = repo_root / relative
        if not path.is_file():
            failures.append(f"missing regression source file: {relative.as_posix()}")
            continue
        content = read_text(repo_root, relative)
        for test_name in test_names:
            if re.search(rf"\bfn\s+{re.escape(test_name)}\s*\(", content) is None:
                failures.append(
                    f"{relative.as_posix()}: missing required regression test `{test_name}`"
                )
    return failures


def verify_regression_script(repo_root: Path) -> list[str]:
    failures: list[str] = []
    path = repo_root / REGRESSION_SCRIPT_REL
    if not path.is_file():
        return [f"missing regression script: {REGRESSION_SCRIPT_REL.as_posix()}"]

    content = path.read_text(encoding="utf-8")
    for test_name in REQUIRED_REGRESSION_TESTS:
        if test_name not in content:
            failures.append(
                f"{REGRESSION_SCRIPT_REL.as_posix()}: missing focused regression `{test_name}`"
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
            print(f"ui-spec-contract: FAIL: {failure}", file=sys.stderr)
        return 1

    print(
        "ui-spec-contract: PASS: verified send/draft/stream spec docs, focused regressions, and contract wiring"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

## Auto-UI Test Assets

The auto-UI test configurations and fixtures are stored under the repository's
`scripts/auto-ui` directory, including:

- `fast.toml`
- `full.toml`
- `multi-width.toml`
- `fast-header.toml`
- `fixtures/` (support data used by the harness)

When you need to run or debug the auto-UI flows, start from those files in
`scripts/auto-ui` and point the harness to the appropriate one.

## Running auto-UI tests

The auto-UI harness is executed from `scripts/` (not `tests/`). Useful entry
points are:

- `scripts/auto-ui-test.sh` for the fast suite
- `scripts/auto-ui-test-full.sh` for full-suite coverage, with optional real prompt-debug verification via `PROMPT_DEBUG_SESSION_ID` and `PROMPT_DEBUG_SESSION_JSON`
- `scripts/auto-ui-test-workspace.sh` for the workspace-picker Enter flow
- `scripts/auto_ui_prompt_debug.sh` for prompt-specific flow debugging
- `scripts/auto-ui-test-prompt-debug.sh` for prompt-debug plus repo-local assertions
- `scripts/auto-ui-test-long.sh` for the full/long prompt scenario

Typical run commands:

- `scripts/auto-ui-test.sh`
- `scripts/auto-ui-test-full.sh`
- `scripts/auto-ui-test-workspace.sh`
- `scripts/auto-ui-test-prompt-debug.sh --session-id <id> --session-json <path> --require-substring 'fn main()'`
- `KEEP_OUT=1 KEEP_HOME=1 scripts/auto-ui-test-long.sh`

# Evidence Gate

**Status**: Current gate definition
**Last updated**: 2026-05-21

This document defines the evidence gate that should decide whether a
`rust-chatbot` change is ready to ship.

The gate is now wired through one repo-local entrypoint backed by a
machine-readable contract registry. The registry is repo-aware, so the
`rust-chatbot` gate refuses to run from a sibling repo root and only interprets
the staged paths for this repo:

- `.github/workflows/engineering-gate.yml`
- `contracts/engineering-gate.toml`
- `scripts/verify-engineering-gate.sh`
- `.githooks/pre-commit`

## Purpose

The merge or release unit for this app should not be "a plausible patch". It
should be a bounded evidence package:

```text
{ contract, regression, replay assertion, sibling-boundary check, implementation }
```

## Required Inputs

The evidence gate should combine these classes of checks:

1. **Repo-local targeted tests**
   Examples: pending-draft, session-reconcile, prompt-debug, and background
   flow regressions.
2. **Replay or artifact assertions**
   Examples: prompt-debug assertion mode and other auto-ui wrapper outputs.
3. **Cross-repo boundary checks**
   Example: `../chatbot-worker/scripts/verify-sibling-builds.sh`
4. **Documentation synchronization**
   Any changed contract must update the owning docs and verification mapping.

## Current Gate Wiring

The current gate is activated from `.githooks/pre-commit`, which runs:

```bash
scripts/verify-engineering-gate.sh
```

That wrapper loads `contracts/engineering-gate.toml`, evaluates the staged file
set against the declared contract globs, enforces required documentation or
verification updates for active contracts, and runs the registered repo-local
checks.

Remote verification now reuses that same entrypoint from
`.github/workflows/engineering-gate.yml` with `--diff-base <sha> --diff-head HEAD`.
Before the gate runs, the workflow hydrates the sibling path-dependency repos
that the repo-local release regressions compile against today:
`chatbot-worker`, `chatbot-ui`, `chatbot-history`, `chatbot-adapter`,
`ai-providers`, `the-markdown-commons`, and `agent-memory`. That keeps CI
path-sensitive even after those folders moved into sibling repos instead of
flattening the gate into one global job.

## Current Component Commands

The current gate registry can dispatch these underlying commands:

- `python3 -m unittest tests.test_engineering_assessment_docs`
- `python3 -m unittest tests.test_forge_plaintext_docs`
- `python3 -m unittest tests.test_auto_ui_assertions`
- `python3 scripts/verify_session_lifecycle_contract.py`
- `python3 scripts/verify_spool_session_contract.py`
- `python3 scripts/verify_history_contract.py`
- `python3 scripts/verify_memory_contract.py`
- `bash scripts/verify-session-lifecycle-regressions.sh`
- `bash scripts/verify-spool-session-regressions.sh`
- `bash scripts/verify-history-regressions.sh`
- `bash scripts/verify-memory-regressions.sh`
- `scripts/auto-ui-test-prompt-debug.sh --assert-only` against
  `tests/fixtures/prompt_debug_assert/`
- `scripts/auto-ui-test-background-flow.sh`
- `../chatbot-worker/scripts/verify-sibling-builds.sh --check --release --repo chatbot-worker --repo __DESKTOP_REPO__`

## Gate Checklist

Before accepting a contract-sensitive change, confirm all of the following:

1. the behavior is described in the owning architecture or verification docs
2. a repo-local regression, assertion, or monitor exists for the changed rule
3. replayable artifacts exist when the bug class depends on runtime state
4. sibling-repo boundaries still validate when the worker/runtime contract
   changes
5. terminal truth still depends on durable artifacts rather than transient UI
   assumptions

## Current First-Class Command Shape

The current first-class entry point runs, in order:

1. contract activation from the staged file set
2. required documentation or verification-file presence checks
3. targeted repo-local commands registered for the active contracts

The same entrypoint can also activate contracts from a git diff range, which is
how the CI workflow selects only the contract-relevant changes for a given push
or pull request.

Replay assertions, focused background-flow regressions, and sibling-boundary
checks are now dispatched automatically from the registry when their owning
contract globs are touched. The background-flow wrapper also pins its own
`CARGO_TARGET_DIR` under `target/auto-ui-background-flow/` so gate-time release
tests do not contend with unrelated cargo runs. The sibling-boundary command is
also narrowed to the worker/desktop repo pair so unrelated `auto-runs`
failures do not block desktop-only contract changes.

## Failure Handling

If a change fails the evidence gate, the next action should be one of:

- add the missing regression or artifact
- update the contract docs
- narrow the code change until the evidence surface is coherent again

"Looks correct in review" is not sufficient for the failure classes this app
has actually encountered.

## Related Docs

- `docs/verification.md`
- `docs/architecture/engineering-assessment.md`
- `docs/architecture/spool-session-contract-versioning.md`

# Engineering Assessment

This guide identifies the current runtime boundaries and how to validate changes.
Historical commit ledgers and completed rollout reports are omitted because they
are not an interface contract.

## Current ownership

`chatbot-ui` owns rendering traits and widgets. `chatbot-adapter` owns provider
command/transport normalization. `chatbot-history` owns durable history and
restore. The app coordinates these boundaries through background work and its
session event queue.

The UI-Lean Research Translation Contract is documented in `docs/verification.md`.
These candidates are now mapped into native regressions in
`docs/architecture/ui-lean-integration-map.md`; send/draft/stream invariants are
in `docs/architecture/ui-lean-send-draft-stream-spec.md`. External model/research
artifacts complement those checks rather than replace them.

## Current Status: Implemented Vs Enforced Vs Still Needed

### Implemented And Enforced Together

Session/draft transitions, terminal reconciliation, spool durability, history
refresh, and memory integration have focused contract scripts and native
regression bundles. Their entry points include:

- `scripts/verify_ui_spec_contract.py`
- `scripts/verify_ui_thread_contract.py`
- `scripts/verify_session_lifecycle_contract.py`
- `scripts/verify_spool_session_contract.py`
- `scripts/verify_history_contract.py`
- `scripts/verify_memory_contract.py`

Existence of a check is not evidence of a successful run. Follow
`docs/verification.md` for prerequisites and the actual commands.

### Implemented But Only Partially Enforced

Fake-provider, headless, and local contract tests cannot establish the behavior
of every installed provider version, desktop environment, or real session.
UI rendering and timing need the appropriate display/automation environment.
Historical restore and memory behavior need representative integration cases
in addition to isolated unit tests.

### Still Needed

Changes at a boundary need new regression coverage for failure, cancellation,
stale results, and replay behavior. Keep performance claims tied to measurements
and preserve the documented limits on replay artifacts and cache ownership.
Use `property-test-plan.md` as a maintenance guide, not a completed-task ledger.

### Follow-Through Artifacts

- `docs/architecture/session-worker-lifecycle-spec.md`
- `docs/architecture/spool-session-contract-versioning.md`
- `docs/architecture/property-test-plan.md`
- `docs/architecture/state-module-segmentation.md`
- `docs/architecture/evidence-gate.md`
- `contracts/engineering-gate.toml` and `scripts/verify-engineering-gate.sh`

The registry groups checks by `command_group`. Select the group relevant to the
changed boundary and record its result; a source commit alone does not prove
that the behavioral contract passed.

# Property Test Plan

**Status**: Current property-test plan
**Last updated**: 2026-05-21

This document defines the next property-oriented verification layer for
`rust-chatbot`.

The repo already has strong example-based regressions. The next step is to add
generated or permuted checks around the small state machines that fail under
ordering pressure rather than under one obvious happy path.

## Purpose

Property tests here are meant to complement, not replace:

- focused unit regressions
- auto-ui wrappers
- replay fixtures
- cross-repo contract checks

They are especially useful for:

- stale result dropping
- pending-draft authority rules
- spool recovery permutations
- transcript replacement and cleanup behavior

## Priority Property Suites

### 1. Pending-Draft Lifecycle Permutations

Target behavior:

- a pending draft becomes authoritative after submit
- non-empty stream chunks clear it atomically
- empty stream chunks do not clear it
- any terminal event clears it
- passive refresh cannot override it while authoritative

Current anchor tests:

- `src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk`
- `src/app/state/session_runtime_tests.rs::apply_stream_to_current_empty_response_keeps_pending_draft`
- `src/app/state/session_runtime_tests.rs::local_terminal_worker_clears_pending_draft`
- `src/app/state/ui_io_tests.rs::selected_session_reload_skips_replacement_when_pending_draft_active`

Suggested generator dimensions:

- stream event kind: none, empty, non-empty
- terminal event kind: none, empty, non-empty, error
- passive reload timing: before stream, between stream chunks, after terminal
- active versus non-active chat

### 2. Worker-Spool Recovery Permutations

Target behavior:

- request-only startup gaps are tolerated briefly and then pruned
- stale local processing flags clear when the spool never fully materializes
- terminal reconcile only trusts durable terminal evidence

Current anchor tests:

- `src/app/state/sync_slots_tests.rs::poll_worker_slots_clears_missing_request_only_spool_processing_state`
- `src/app/state/session_runtime_terminal_persistence_tests.rs::local_terminal_worker_persists_session_before_worker_cleanup`

Suggested generator dimensions:

- presence or absence of `request.json`, `state.json`, `result.json`
- worker pid alive or dead
- parent pid alive or dead
- terminal state classification

### 3. Transcript Merge And Cleanup Permutations

Target behavior:

- streamed transcript preambles are preserved when needed
- reasoning-only terminal bodies do not clobber useful visible output
- final transcript cleanup does not duplicate or erase user-visible state

Current anchor tests:

- `src/app/state/session_runtime_terminal_persistence_tests.rs::local_terminal_worker_keeps_existing_visible_assistant_when_terminal_response_is_think_only`
- prompt-debug assertion tests in `tests/test_auto_ui_assertions.py`

Suggested generator dimensions:

- stream shape: plain text, transcript preamble, mixed tool lines
- terminal shape: empty, body-only, think-only, duplicate-tail
- provider family: adapter-backed versus Forge/plaintext

### 4. Stale Result Rejection Permutations

Target behavior:

- older async results never override newer authoritative state
- passive and explicit reload paths stay distinct

Current anchor tests:

- `src/app/state/ui_io_tests.rs` stale-request cases
- `docs/architecture/broker-sync.md` passive-versus-explicit refresh rules

Suggested generator dimensions:

- request ids in or out of order
- targeted session active versus inactive
- pending-draft present versus absent

## Acceptance Rule

A property suite is only worth adding if it satisfies at least one of:

- it covers permutations that are expensive to enumerate manually
- it finds a minimal counterexample that becomes a new fixed regression
- it protects a state-machine invariant already documented elsewhere in the repo

## Native Translation Rule

Property tests must remain native `rust-chatbot` checks. They are not a
replacement for the current docs or for `ui-lean` research models.

If a `ui-lean` or other external model identifies a useful property, the final
accepted artifact should still be a repo-local test, assertion, or monitor.

## Related Docs

- `docs/architecture/session-worker-lifecycle-spec.md`
- `docs/architecture/ui-lean-send-draft-stream-spec.md`
- `docs/verification.md`

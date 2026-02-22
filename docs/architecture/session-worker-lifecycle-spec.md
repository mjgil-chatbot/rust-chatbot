# Session And Worker Lifecycle Spec

**Status**: Current lifecycle specification
**Last updated**: 2026-05-21

This document defines the current session and worker lifecycle contract for
`rust-chatbot` at the level of local state transitions and durable spool
artifacts.

It builds on the current runtime model in `overview.md` and the durable spool
contract in `worker-spool.md`, but it expresses the lifecycle as an explicit
state machine so boundary bugs can be reasoned about as transition failures.

## Purpose

The lifecycle contract exists to keep three sources of state aligned:

- active in-memory UI state
- worker spool artifacts under `~/.rust-chatbot/workers/<chat_id>/`
- persisted session state on disk

Most high-severity regressions in this app happen when those three surfaces
disagree about which turn phase is authoritative.

## Roles

| Role | Responsibility |
| --- | --- |
| UI session state | Records the user's current chat, pending draft, focus intent, and visible transcript |
| Worker spool | Acts as the durable source of truth for an in-flight provider run |
| Worker process | Produces `state.json`, `events.jsonl`, `output`, and `result.json` |
| Reconcile leader | Applies terminal spool state back into the session and removes the spool directory |

## State Machine

The lifecycle should be treated as the following state machine for one chat
turn.

### 1. `Idle`

No active worker turn exists for the chat.

Expected state:

- no authoritative pending draft for the chat
- no live worker spool directory for the chat
- input is available unless another chat is processing

### 2. `PendingDraft`

The user submitted a valid message and the app has accepted it locally, but no
real assistant stream has yet taken over.

Entry trigger:

- `send_message_text(...)`

Expected state:

- user turn appended to the current session
- `pending_draft` set for the chat
- passive refresh cannot replace the current chat while this draft is
  authoritative

### 3. `RequestWritten`

The app has prepared the worker request and written `request.json`.

Expected state:

- worker spool directory exists
- `request.json` exists
- missing `state.json` is tolerated temporarily as a startup gap

### 4. `Running`

The worker has claimed the job and is producing spool state.

Entry trigger:

- worker writes initial `state.json`

Expected state:

- `state.json` reflects `Running`
- `events.jsonl` and `output` may advance incrementally
- the UI may hydrate from `output` or the event log while the worker is live

### 5. `StreamingVisible`

At least one real assistant chunk is visible for the active chat.

Expected state:

- the optimistic pending draft has been cleared atomically
- the real assistant row or cached response is now authoritative for the turn

### 6. `TerminalPendingReconcile`

The worker has ended and produced terminal spool state, but the reconcile path
has not yet fully persisted that state back into the session.

Entry trigger:

- worker writes terminal `result.json` and terminal `state.json`

Expected state:

- terminal error or final response is durable in the spool
- the pending draft for the chat must be cleared even if the final response is
  empty

### 7. `Reconciled`

The terminal spool state has been applied to the session and local processing
state has been cleared.

Expected state:

- current or persisted session reflects the final assistant or error state
- input focus has been requested back for the chat
- processing markers for the chat are gone

### 8. `PrunedStartupGap`

Special recovery state for a stale request-only spool directory.

Entry trigger:

- `request.json` exists
- `state.json` never appears within the grace window

Expected state:

- stale spool directory is removed
- local processing flags are cleared so the composer does not remain disabled

### 9. `RecoveredError` / `RecoveredStopped`

The worker path terminated abnormally or was explicitly stopped, but the UI has
still produced a recoverable terminal state.

Expected state:

- draft authority is gone
- the user is not left in a permanent in-flight state
- terminal classification matches stop/error behavior

## Core Invariants

1. `request.json` may exist before `state.json`, but only within the startup
   grace window.
2. `Running` is not accepted as terminal truth; terminal acceptance requires
   `result.json` or a recovery path that classifies the failure.
3. Once a non-empty stream chunk takes over, the synthetic pending draft must
   no longer be authoritative for that chat.
4. Any terminal event for the active chat clears the pending draft even if the
   response body is empty.
5. Reconciliation is single-writer: either the spawning UI instance or the
   chosen leader applies terminal spool state.
6. Automatic current-session replacement is transcript-gated rather than
   timestamp-gated.

## Native Coverage Anchors

Current repo-local checks that cover the lifecycle include:

- `src/app/state/ui_spec_lifecycle_tests.rs::submit_valid_creates_pending_draft_and_requests_focus`
- `src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk`
- `src/app/state/session_runtime_tests.rs::local_terminal_worker_clears_pending_draft`
- `src/app/state/session_runtime_tests.rs::local_terminal_worker_clears_pending_draft_even_on_empty_response`
- `src/app/state/session_runtime_tests.rs::remote_terminal_update_clears_pending_draft`
- `src/app/state/session_runtime_terminal_persistence_tests.rs::local_terminal_worker_persists_session_before_worker_cleanup`
- `src/app/state/sync_slots_tests.rs::poll_worker_slots_clears_processing_for_stale_request_only_spool_entry`
- `src/app/state/ui_io_tests.rs::selected_session_reload_skips_replacement_when_pending_draft_active`
- `src/app/state/ui_io_reload_guard_tests.rs::selected_session_reload_drops_divergent_newer_snapshot_for_current_chat`
- `src/app/state/ui_io_reload_guard_tests.rs::selected_session_reload_accepts_strict_extension_even_with_equal_timestamp`

The reconciled assistant body now also has explicit coverage for persisted
tool-output preambles so completed turns can preserve streamed transcript rows
without letting those rows break the final markdown body:

- `src/app/state/helpers_tests.rs::upsert_assistant_response_preserves_transcript_and_strips_think_block`
- `src/app/ui/message_tests.rs::normalized_markdown_content_sanitizes_real_extracted_tool_transcript_sample`
- `src/app/ui/message_list/tests/render_regressions.rs::extracted_generic_tool_transcript_sample_does_not_overflow_or_hide_body_markdown`

## Current Native Enforcement

The current repo-local enforcement surface for this lifecycle contract is:

- `scripts/verify_session_lifecycle_contract.py`
- `scripts/verify-session-lifecycle-regressions.sh`

The verifier keeps the lifecycle spec, verification mapping, and focused
release regression bundle aligned. The regression script runs the exact
release-mode tests that currently back lifecycle entry, stream takeover,
terminal cleanup, request-only startup-gap pruning, and guarded current-session
replacement behavior.

## Related Docs

- `docs/architecture/overview.md`
- `docs/architecture/worker-spool.md`
- `docs/verification.md`

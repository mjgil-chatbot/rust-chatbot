# UI-Lean Send/Draft/Stream Interaction Spec

**Status**: Initial bounded interaction spec
**Last updated**: 2026-05-21

This document defines the first small `ui-lean` candidate interaction for
`rust-chatbot`: the foreground chat lifecycle from user submit through either
streamed assistant output or a terminal failure/cleanup path.

It is intentionally narrow. The goal is not to model provider semantics or
worker spool persistence. The goal is to model the local UI/state transitions
that are currently protected by pending-draft, stream, and terminal regressions.

## Scope

Modeled concerns:

- valid submit from the active chat
- optimistic pending-draft marker creation
- first real assistant chunk replacing that marker
- terminal cleanup when the turn ends before or after streaming
- focus restoration and recoverable idle state
- passive reload guard behavior while the pending draft is authoritative

Explicitly out of scope:

- provider resume/session identity semantics
- spool directory durability
- multi-window broker fanout
- exact network timing
- long-lived background worker recovery after process crashes

## State Variables

The bounded model should track at least these variables:

- `active_chat_selected`
- `pending_draft_for_active_chat`
- `processing_active_chat`
- `assistant_row_visible`
- `terminal_result_seen`
- `send_failed`
- `input_focus_requested`
- `passive_reload_attempted`

The key interpretation is:

- `pending_draft_for_active_chat` is a transient pre-stream authority signal
- it is not the same thing as full worker processing
- once processing/streaming visibly takes over, the synthetic draft row no
  longer represents the authoritative assistant state

That matches the current desktop behavior where `should_show_pending_draft(...)`
returns `false` while processing is active.

## Events

The first model should include these events:

1. `submit_valid`
2. `stream_chunk_non_empty`
3. `stream_chunk_empty`
4. `local_terminal_non_empty`
5. `local_terminal_empty`
6. `remote_terminal_non_empty`
7. `remote_terminal_empty`
8. `send_failed`
9. `passive_reload_current_chat`
10. `switch_to_other_chat`

## Transition Intent

### `submit_valid`

- appends the user turn
- sets `pending_draft_for_active_chat = true`
- requests input focus restoration
- makes the active in-memory chat authoritative over passive refresh for that
  chat until the draft clears

Current native anchor:

- `src/app/state/messaging.rs::send_message_text`

### `stream_chunk_non_empty`

- creates or updates the visible assistant row
- clears `pending_draft_for_active_chat` atomically
- transfers authority from the synthetic draft state to the real assistant row

Current native anchors:

- `src/app/state/session_runtime.rs::apply_stream_to_current`
- `src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk`

### `stream_chunk_empty`

- must not clear the pending draft on its own
- must not falsely claim that a real assistant row has taken over

Current native anchor:

- `src/app/state/session_runtime_tests.rs::apply_stream_to_current_empty_response_keeps_pending_draft`

### `local_terminal_*` and `remote_terminal_*`

- always clear the pending draft for the targeted chat
- must leave the chat in a recoverable idle state even if the terminal response
  body is empty
- must request input focus restoration

Current native anchors:

- `src/app/state/session_runtime.rs::handle_remote_stream_terminal_update`
- `src/app/state/session_runtime_tests.rs::local_terminal_worker_clears_pending_draft`
- `src/app/state/session_runtime_tests.rs::local_terminal_worker_clears_pending_draft_even_on_empty_response`
- `src/app/state/session_runtime_tests.rs::remote_terminal_update_clears_pending_draft`

### `send_failed`

- clears the pending draft
- appends a visible error result instead of leaving the composer in limbo

Current native anchor:

- `src/app/state/ui_io_tests.rs::send_failed_clears_pending_draft_and_appends_error`

### `passive_reload_current_chat`

- must not replace the authoritative current chat while
  `pending_draft_for_active_chat = true`
- may proceed once that authority signal clears

Current native anchors:

- `src/app/state/ui_io_tests.rs::selected_session_reload_skips_replacement_when_pending_draft_active`
- `src/app/state/ui_io_tests.rs::selected_session_reload_proceeds_when_no_pending_draft`

### `switch_to_other_chat`

- must hide the pending-draft indicator for the newly active chat
- must not clear the old chat's pending-draft marker just because the view
  switched away

Current native anchors:

- `src/app/state/session_runtime_tests.rs::switching_chats_preserves_pending_draft_for_original_chat`
- `src/app/ui/message_list/tests/render_regressions.rs::render_with_pending_draft_for_other_chat_hides_indicator`

## Required Properties

### Property 1: No Permanent Disabled-Composer State

After `submit_valid`, every reachable path must eventually do one of the
following:

- produce a real visible assistant row, or
- reach a terminal failure/cleanup state that clears the pending draft and
  returns the chat to a recoverable idle condition

The model does not need to prove remote provider success. It only needs to
prove that local UI authority does not get stuck in a limbo state forever.

### Property 2: Real Assistant Output Replaces Synthetic Draft Authority

Once a non-empty stream chunk becomes visible for the active chat,
`pending_draft_for_active_chat` must become false in the same transition class.

This prevents duplicate "Thinking..." plus real assistant ownership for the
same turn.

### Property 3: Terminal Cleanup Clears Pending Draft Even With Empty Output

Any terminal event for the active chat must clear the pending draft regardless
of whether the final response body is empty.

This captures the cleanup rule that prevents a stranded optimistic row.

### Property 4: Passive Refresh Cannot Override Authoritative Draft State

While `pending_draft_for_active_chat = true`, a passive reload of the current
chat must not replace the in-memory current session.

### Property 5: Rendering Alone Does Not Mutate Draft Ownership

Pure render passes must not clear the pending draft without a stream or
terminal event.

Current native anchor:

- `src/app/ui/message_list/tests/render_regressions.rs::render_with_pending_draft_does_not_clear_or_panic`

## Expected `ui-lean` Outputs

The first `ui-lean` experiment for this interaction should produce:

1. a bounded state model for the variables and events above
2. either a proof sketch/check or a counterexample trace for the required
   properties
3. a short invariant summary suitable for `rust-chatbot` maintainers
4. any missing native regression implied by a newly discovered counterexample

## Current Native Enforcement

The current repo-local enforcement surface for this bounded interaction is:

- `scripts/verify_ui_spec_contract.py`
- `scripts/verify-ui-spec-regressions.sh`
- `src/app/state/ui_spec_lifecycle_tests.rs::submit_valid_creates_pending_draft_and_requests_focus`
- `src/app/state/ui_spec_lifecycle_tests.rs::local_terminal_empty_response_requests_focus_and_clears_pending_draft`
- `src/app/state/ui_spec_lifecycle_tests.rs::send_failed_returns_chat_to_recoverable_idle_state`

The same bounded surface now also includes the transcript-summary sanitizer and
render regressions that protect persisted streamed-tool preambles from
reinterpreting the final markdown answer body:

- `src/app/ui/message_tests.rs::normalized_markdown_content_sanitizes_real_extracted_tool_transcript_sample`
- `src/app/ui/transcript_summary.rs` transcript-only sanitizer tests
- `src/app/ui/message_list/tests/render_regressions.rs::extracted_generic_tool_transcript_sample_does_not_overflow_or_hide_body_markdown`

The verifier keeps the spec, docs, and focused regression bundle aligned. The
regression script runs the exact release-mode tests that currently back the
spec's pending-draft, stream, terminal-cleanup, passive-reload, and pure-render
properties.

## Success Condition For This Spec

This spec is considered integrated only when a `ui-lean` run against it either:

- confirms that the existing native regressions already cover the intended
  invariant set, or
- exposes a counterexample that becomes a new `rust-chatbot` regression test,
  assertion, or monitor

Without that final native artifact, the spec remains useful research but not a
completed verification input to the app.

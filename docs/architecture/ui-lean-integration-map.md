# UI-Lean Integration Map

**Status**: Current integration map
**Last updated**: 2026-05-21

This document maps the proposed `~/git/ui-lean` experiments onto the existing
`rust-chatbot` regression and monitoring surface.

The integration rule is strict:

```text
ui-lean model
-> checked property or counterexample
-> distilled invariant
-> native rust-chatbot test, assertion, or monitor
```

If a `ui-lean` experiment does not produce a native `rust-chatbot` artifact, it
remains research and is not part of the app's accepted verification surface.

## Candidate Experiment Matrix

| Candidate interaction | Why it is a good fit for `ui-lean` | Current native anchors | Expected native output |
| --- | --- | --- | --- |
| Send/draft/stream/terminal lifecycle | Finite local state machine with clear UI ownership and already-known edge cases around pending drafts and terminal cleanup | `src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk`, `src/app/state/session_runtime_tests.rs::local_terminal_worker_clears_pending_draft`, `src/app/state/session_runtime_tests.rs::remote_terminal_update_clears_pending_draft`, `src/app/state/ui_io_tests.rs::send_failed_clears_pending_draft_and_appends_error`, `src/app/ui/message_list/tests/render_regressions.rs::render_with_pending_draft_does_not_clear_or_panic`, `src/app/state/ui_spec_lifecycle_tests.rs::submit_valid_creates_pending_draft_and_requests_focus`, `scripts/verify_ui_spec_contract.py`, `scripts/verify-ui-spec-regressions.sh`, `contracts/engineering-gate.toml` | One bounded interaction spec backed by a static contract verifier, a focused release regression bundle, and any new regression covering missing submit/focus, terminal cleanup, or duplicate-row counterexamples |
| Session switch versus passive refresh | Finite control interaction where authoritative state must outrank passive background updates | `src/app/state/ui_io_tests.rs::selected_session_reload_skips_replacement_when_pending_draft_active`, `src/app/state/ui_io_tests.rs::selected_session_reload_proceeds_when_no_pending_draft`, `src/app/state/ui_io_tests.rs::selected_session_reload_proceeds_when_pending_draft_is_for_other_chat`, `src/app/state/ui_io_tests.rs::selected_session_reload_without_sidebar_patch_preserves_existing_sidebar_state`, `src/app/state/ui_io_async_tests.rs::spawn_load_session_async_skips_sidebar_snapshot_reload_on_critical_path`, `src/app/state/session_runtime_tests.rs::switching_chats_preserves_pending_draft_for_original_chat`, `src/app/state/session_runtime_tests.rs::switching_chats_loads_each_session_height_cache_in_background`, `docs/architecture/broker-sync.md` passive-vs-explicit refresh contract | A concise invariant set for "passive refresh never replaces authoritative current-chat state", plus native regressions that keep session activation immediate by separating transcript loads from sidebar relists while height-cache hydration remains asynchronous |
| Background-offload discipline | Bounded event-path model with explicit "must stay off frame path" intent and existing timing instrumentation | `src/app/state/messaging_flow_tests.rs::check_for_response_drains_worker_workspace_and_history_background_channels_together`, `src/app/state/session_runtime_tests.rs::switching_chats_loads_each_session_height_cache_in_background`, `src/app/ui/main_panel.rs` cached MCP-config status plus sidebar ordering trace fields, `scripts/auto-ui-test-background-flow.sh`, `scripts/auto-ui-assertions.py` session-switch and new-chat ordering checks, `docs/architecture/overview.md` background-channel notes, `src/app/state/background_timing.rs`, `src/app/ui/layout_log.rs`, `scripts/verify_ui_thread_contract.py`, `.githooks/pre-commit`, `contracts/engineering-gate.toml`, `tests/test_ui_thread_contract.py` | A native assertion checklist for allowed frame-path work, async offloads for formerly inline layout-log, render-time MCP config reads, and session-switch height-cache reads, plus auto-ui ordering assertions for "render first, hydrate later" and "new chat shows up in the sidebar immediately" alongside a pre-commit/static contract for future UI-thread regressions |

The current `ui-thread` verifier intentionally does not depend on a single
fixed sibling-repo layout. It resolves the upstream `ui-lean` checkout from
`--ui-lean-root`, `RUST_CHATBOT_UI_LEAN_ROOT`, nearby repo candidates, or
`~/git/ui-lean` so repository moves do not silently disable the gate.

## Current Recommended Entry Point

The first `ui-lean` experiment should target the send/draft/stream/terminal
lifecycle because it has all of the following:

- a compact state space
- visible user-facing failure modes
- a well-defined transition from optimistic UI state into streamed or terminal
  assistant state
- multiple existing regressions that already pin the current behavior

The initial spec for that interaction now lives in
`docs/architecture/ui-lean-send-draft-stream-spec.md`.

## Native Acceptance Rule

A `ui-lean` experiment is complete only when all of the following exist:

1. a checked model or counterexample trace
2. a human-readable invariant statement
3. a repo-local `rust-chatbot` artifact that enforces or watches that invariant

Acceptable native artifacts include:

- Rust unit or integration tests
- Python doc or script assertions
- auto-ui assertions
- runtime latency or state monitors
- verification-doc entries that map the invariant to named tests
- pre-commit or engineering-gate checks that reject blocking UI-thread work

## Non-Goals

This integration map intentionally excludes:

- provider resume protocols
- worker spool file durability semantics
- cross-repo orchestration contracts
- open-world remote latency guarantees

Those areas are currently better captured by the spool, provider, and sibling-
repo verification documents than by a finite local UI model.

# State Module Segmentation Plan

**Status**: Current segmentation follow-through
**Last updated**: 2026-05-21

This document turns the high-level refactoring guidance into an explicit split
plan for the largest state and control modules in the current tree.

It is a concrete follow-through to `refactoring-plan.md`, not a replacement for
it.

## Current Pressure Points

The strongest current production-code hotspots are:

- `src/bin/chatbot_ctl/messaging.rs` at 936 lines
- `src/app/state/session_runtime.rs` at 768 lines
- `src/app/state/history.rs` at 758 lines
- `src/app/ui/message_list.rs` at 749 lines
- `src/app/state/messaging.rs` at 718 lines
- `src/memory_job.rs` at 713 lines
- `src/app/state/session.rs` at 658 lines

The file-size guard in `tests/file_size_limits.rs` enforces an 800-line cap.
The goal is not merely to stay under the cap, but to move these files away from
the threshold and align ownership seams with the actual runtime contracts.

## Segmentation Targets

### `src/app/state/messaging.rs`

Split into:

- `send_pipeline.rs`
  submit validation, pending-draft setup, send preparation
- `event_loop.rs`
  ingress phases and deterministic queue replay
- `stale_result_policy.rs`
  request-id and authoritative-target stale-result rejection
- `worker_launch.rs`
  explicit send-time worker binary and launch preparation

### `src/app/state/session_runtime.rs`

Split into:

- `stream_apply.rs`
  non-terminal stream application and pending-draft handoff
- `terminal_reconcile.rs`
  local and remote terminal update handling
- `session_reload_guard.rs`
  transcript-gated replacement and reload safety
- `worker_hydration.rs`
  active-worker hydration and running-session reconstruction

### `src/app/state/history.rs`

Split into:

- `history_refresh.rs`
- `history_checkpoint.rs`
- `history_snapshot.rs`
- `history_notice.rs`

Keep repo-capture and sync logic in their existing dedicated helper modules.

### `src/app/ui/message_list.rs`

Split into:

- `message_viewport.rs`
- `message_height_cache.rs`
- `message_scroll.rs`
- `message_rows.rs`

Keep one small coordinator that assembles the render flow.

### `src/bin/chatbot_ctl/messaging.rs`

Split into:

- `send_request.rs`
- `worker_spawn.rs`
- `broker_notifications.rs`
- `restart_or_recover.rs`

### `src/memory_job.rs`

Split into:

- `memory_job_cli.rs`
- `memory_job_spawn.rs`
- `memory_job_execute.rs`
- `memory_job_fallback.rs`

## Ownership Rules

Each segmented module should answer one of these questions cleanly:

- who owns the authoritative state transition?
- who owns durable artifact interpretation?
- who owns background IO orchestration?
- who owns purely presentational render decisions?

If a file cannot be described in one of those terms, it is still too broad.

## Test Topology Follow-Through

Production splits should be matched by smaller local test groupings:

- stream and terminal rules beside their production modules
- stale-result policy tests beside stale-result policy code
- render-only draft tests beside message-list rendering helpers
- `tests/` reserved for cross-module or end-to-end behaviors

## Acceptance Rule

A segmentation step is not complete just because the line count dropped. It is
complete only when:

1. the new module owns one coherent contract boundary
2. tests move closer to that boundary
3. no user-visible behavior changed without a dedicated regression or bug fix

## Related Docs

- `docs/architecture/refactoring-plan.md`
- `docs/architecture/session-worker-lifecycle-spec.md`
- `docs/verification.md`

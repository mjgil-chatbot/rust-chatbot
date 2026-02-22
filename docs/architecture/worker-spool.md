# Worker Spool

This document describes the durable job spool used for in-flight provider
responses.

## Purpose

The spool is the source of truth for an active model response. It allows the UI
to survive crashes, restarts, and multiple windows without losing the current
assistant output.

## Layout

Each active chat response gets one spool directory:

```text
<shared_infra_dir>/
  workers/
    <chat_id>/
      request.json
      state.json
      events.jsonl
      output
      result.json
      transport-errors.log
```

`shared_infra_dir()` resolves to `~/.rust-chatbot`. Older machines can still
fall back to `~/.claude-desktop` during migration when that legacy store
already exists (see `../chatbot-worker/src/config/paths.rs::shared_infra_dir()`).

## Files

### `request.json`

Written by the UI before the worker starts. Contains provider, query, optional
system prompt, optional resume session ID, the rendered command string, the
chat ID, and `schema_version`.

### `state.json`

Written by the worker on startup and after each streamed update. Tracks:

- `schema_version`
- `chat_id`
- `provider`
- `worker_pid`
- `parent_pid`
- `state`
- `started_at`
- `updated_at`
- `output_len`
- `sequence`

### `output`

The latest full accumulated assistant response as UTF-8 text.

### `events.jsonl`

The durable structured stream log. Workers append newline-delimited
`StreamEvent` JSON records here while the provider is running.
Oversized tool input/output payloads are spool-bounded before write: small
payloads are stored verbatim, while large tool payloads are replaced with a
truncated preview plus original byte count so one pathological tool result
cannot balloon the spool into hundreds of megabytes or get replayed upstream.
The session MCP `inspect_session` surface also scans these events and reports
any truncated tool payload previews back to operators, alongside the current
spool state and terminal error, so oversized tool output can be diagnosed
without opening the raw spool files.

### `result.json`

Written once on terminal completion. Contains the provider session ID, final
summary, terminal error when present, and `schema_version`.

When a provider CLI fails, the worker persists the concrete CLI error text it
received instead of a generic placeholder. That lets the UI and saved session
history surface the actual provider-side failure summary after spool
reconciliation.

### `transport-errors.log`

Written only when the adapter-backed worker retries or exhausts a retryable
transport-marked failure. Each line records a timestamped retry diagnostic for
that chat's run, giving operators a stable per-chat transport log in the spool.

Current spool readers now validate `schema_version` on `request.json`,
`state.json`, and `result.json`. Missing version fields still deserialize as
the current baseline `v1` for compatibility with older on-disk artifacts, while
unknown future versions are rejected as unsupported contract inputs.

For the current desktop-owned contract baseline, that means:

- schema_version = 1 on `request.json`
- schema_version = 1 on `state.json`
- schema_version = 1 on `result.json`

## Current Native Enforcement

The current repo-local enforcement surface for the spool/session contract is:

- `scripts/verify_spool_session_contract.py`
- `scripts/verify-spool-session-regressions.sh`

The verifier keeps this spool document, the versioning policy, and the
verification mapping aligned. The focused release bundle exercises the current
desktop-reader contract around unsupported future schema versions, durable
terminal persistence ordering, request-only spool pruning, provider-session-id
replacement on terminal reconciliation, think-only terminal payload handling,
and session MCP inspection of truncated spool payload previews.

## Lifecycle

1. `send_message()` saves the user message and writes `request.json`.
2. The UI spawns `chatbot-worker <chat_id>` detached with `setsid`.
3. The worker writes initial `state.json` with `Running`.
4. The worker runs the provider CLI.
5. On each chunk, the worker atomically rewrites `output` and bumps `state.json`.
6. On success, stop, or error, the worker writes `result.json` and a terminal state.
7. The UI reconciles the session from spool data and removes the spool directory.

## Recovery Rules

- `Running` plus a live PID means the next UI instance resumes watching the job.
- `Running` plus a dead PID becomes an error recovery path.
- `Done`, `Stopped`, and `Error` are reconciled from `output` and `result.json`.
- While a job is running, the UI keeps a per-chat in-memory transcript cache.
  It seeds that cache from `output` when present, otherwise from a one-time
  `events.jsonl` reconstruction, and then appends only newly read events on
  later polls instead of rebuilding the full transcript every cycle.
- A spool directory without `state.json` is treated as a transient startup gap
  for up to five seconds because the UI writes `request.json` before the worker
  claims the job.
- If that grace period expires and `state.json` still never appears, spool
  scans prune the stale directory instead of warning on every poll cycle.
- The UI poll loop treats those stale request-only directories as missing spool
  state after the prune and clears any lingering local `processing` flags so
  the current chat regains input focus instead of staying stuck in a loading
  state.
- If a Forge/plaintext stream already rendered transcript bullet lines and the
  provider's final payload drops back to body-only content, session persistence
  keeps the last streamed transcript preamble instead of overwriting it away.
- For adapter-backed standard providers, terminal persistence trims any trailing
  `TextDelta` suffix by concatenating the final streamed delta chunks before it
  compares them to the provider's final answer. This avoids duplicating the
  final reply when chunk boundaries do not match the finalized text layout.
- The same terminal persistence path also strips a leading `<think>...</think>`
  reasoning block from the provider's final body before it writes durable
  assistant output. If nothing visible remains after that strip, the UI keeps
  any already-streamed assistant transcript instead of overwriting it with a
  reasoning-only terminal payload.
- A `Running` job whose `parent_pid` is gone is adoptable by the reconcile
  leader even while the worker PID is still alive. This keeps the chat
  stoppable after the launcher window exits and avoids a permanently disabled
  composer.

Terminal reconciliation is single-writer:

- the spawning UI reconciles while it is alive
- otherwise the lowest live UI PID becomes the reconciliation leader

## Atomicity

The worker updates `state.json`, `output`, and `result.json` with temp-file
rename writes, so readers only see complete versions.

## Operational Notes

- `chatbot-ctl workers` lists spool jobs
- `chatbot-ctl stop-workers` sends `SIGTERM` to active workers
- `chatbot-ctl gc` removes stale terminal or dead-running jobs
- `chatbot-ctl gc` also reports and removes stale request-only spool entries
  with `state:MISSING_STATE` when `request.json` ages past the same five-second
  startup grace period used by automatic spool scans.
- `chatbot-worker` now emits dense `rust-debug` traces around request load,
  provider-call thread start/finish, per-chunk output/state writes,
  intermediate `result.json` writes, and terminal result/state persistence so
  spool and UI reconciliation races can be reconstructed from one trace.
- Adapter-backed worker launches now put `agent-adapter-cli` in its own process
  group and poll the stop flag even while stdout is idle, so a UI stop request
  can terminate a silent provider run instead of waiting for the next log line.
- Adapter-backed launches also drain adapter `stderr` concurrently with stdout,
  and any adapter hard failure now persists terminal `Error` state through the
  same spool finalization path instead of leaving `state.json` behind in
  `Running`.
- Adapter-backed launches now retry one transport-marked failure internally
  when the failed attempt emitted no assistant text yet. Those retry
  diagnostics are written to `transport-errors.log`, mirrored into the rotating
  app log, and summarized into `events.jsonl` as diagnostic live-summary rows.
- That same retry classification now also accepts pre-text raw provider
  disconnect errors such as Codex websocket closures before
  `response.completed`, so transient provider transport drops can be retried
  even when the adapter exits before emitting a structured `RunFailed` event.
- When an adapter-backed run exits with a concrete terminal failure after
  streaming partial assistant text, the persisted `result.json` error now
  prefers the explicit adapter/stderr failure detail over the partial streamed
  output. That keeps saved session error rows from echoing progress text as the
  terminal error message.
- If an adapter-backed child ignores the initial stop `SIGTERM`, the worker now
  escalates to `SIGKILL` after a short grace period and persists the terminal
  spool state as `Stopped` for user cancellations instead of leaving the job in
  `Running` or misclassifying it as `Error`.
- The worker and provider adapters also log the boundary between
  `worker stream channel closed`, `stdout stream closed`, and `child.wait()`
  completion, which distinguishes “still receiving provider chunks” from
  “provider stdout ended but process exit is still pending”.
- Session writes now also log their write source, message counts, last-message
  role, and history fingerprint before persisting, so a later overwrite can be
  tied back to the exact writer path instead of inferred from the damaged file.
- Automatic replacement of the active in-memory chat is transcript-gated: the
  loaded disk session must either exactly match the current transcript or be a
  strict extension of it. `updated_at` alone is not authoritative enough to
  justify replacing the current session.
- Delete fallback also strips the just-deleted session ID from the immediate
  reload snapshot before reselecting a surviving chat, so a soft-deleted
  session file cannot briefly reappear as an orphaned visible row.

## Related Docs

- [Overview](overview.md)
- [Broker And Sync](broker-sync.md)

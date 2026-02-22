# Stale Provider Session Regression

## Context

MiniMax desktop chats can persist a provider-native resume handle such as
`minimax_session_id` long after the upstream provider has discarded that
session. When the Claude-compatible ACP bridge resumes with that stale handle,
the provider returns `Session not found`, the bridge falls back to
`session/create`, and rust-chatbot must replace the stored provider session id
with the fresh value returned by the worker.

## Implementation

- Shared state logic now routes provider-session updates through
  `replace_provider_session_id(...)` in
  `src/app/state/helpers.rs`.
- The remote terminal echo path in `session_runtime.rs` and both local worker
  completion paths in `sync.rs` and `sync_slots.rs` now overwrite any existing
  provider session id when the worker returns a fresh non-empty `session_id`.
- Focused release tests cover the helper itself plus both runtime paths that
  previously kept stale provider session ids alive.

## Auto-UI Verification

`scripts/auto-ui-test-stale-provider-session.sh` provides a real-UI regression
wrapper for this flow. It:

1. Backs up a saved desktop session JSON.
2. Seeds a synthetic stale provider session id into the provider-specific field
   such as `minimax_session_id`.
3. Runs the existing `prompt_debug` auto-ui flow against that live session.
4. Asserts the saved session JSON now contains a different non-empty provider
   session id after the bridge falls back from `session/resume` to
   `session/create`.

On failure, the wrapper restores the original session JSON so the manually
seeded stale provider session id does not linger on disk.

The shared `prompt_debug` assertion layer underneath that wrapper also supports
negative transcript checks now, so real-provider UI regressions can reject raw
tool labels such as MiniMax `| Shell` lines when normalized search activity is
expected instead.

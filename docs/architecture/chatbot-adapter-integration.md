# Chatbot-Adapter Integration

CLI-backed turns use the sibling `chatbot-adapter` workspace through
`chatbot-worker`. This is the implemented execution boundary, rather than a
provider-by-provider migration plan.

## Ownership

- `rust-chatbot` owns UI state, session selection, request submission, and
  reconciliation of worker results.
- `chatbot-worker` owns the durable spool and the adapter bridge under
  `src/provider_bridge/`.
- `chatbot-adapter` owns provider command construction, transport execution,
  normalized events, cancellation, and provider capability checks.
- `ai-providers` retains the lower-level provider runtime used by callers that
  use its streaming API directly.

## Request and event flow

The app writes a worker request into the configured spool. The worker resolves
`agent-adapter-cli`, maps the request's provider, workspace, prompt, and session
options, and consumes the resulting event stream. The worker maintains
`state.json`, `events.jsonl`, `output`, and terminal `result.json`. The UI polls
cached worker state and reconciles session identity when completion arrives.

See [worker spool](worker-spool.md) and the sibling adapter's
[CLI documentation](../../../chatbot-adapter/docs/cli.md) for the concrete
formats and command surface. Provider-specific compatibility and MCP setup
remain in `chatbot-worker`; they should not be duplicated inside UI rendering.

## Build and verification

```bash
cargo build --release --manifest-path ../chatbot-adapter/Cargo.toml -p agent-adapter-cli
cargo build --release --manifest-path ../chatbot-worker/Cargo.toml
cargo build --release
```

Build the adapter with the worker and install their binaries together, or use
the documented companion-binary environment overrides. Provider CLIs must be
installed and authenticated separately. Tests use fake provider processes;
real-provider tests require explicit credentials and opt-in.

The [verification contract](../verification.md) identifies the focused checks
for provider parity, spool durability, cancellation, and session reconciliation.

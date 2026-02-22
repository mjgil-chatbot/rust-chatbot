# Architecture Overview

`rust-chatbot` is the desktop composition layer for an egui chat interface,
detached worker execution, and durable conversation history.

## Layered Architecture

```text
chatbot-ui                  UI traits, widgets, and message rendering
    |
rust-chatbot                AppState, sessions, event queue, controller, broker
    |
chatbot-worker              Durable spool, execution lifecycle, provider bridge
    |
chatbot-adapter              CLI configuration, transports, normalized events
    |
external provider CLIs      Installed and authenticated by the operator
```

`the-markdown-commons` supplies markdown parsing/rendering. `chatbot-history`
supplies history, snapshot, and restore crates. `history-auditor` supplies
transcript auditing and MCP handlers. `ai-providers` is the reusable lower-level
provider runtime. All eight source repositories use sibling Cargo paths.

## Binaries

| Binary | Role |
| --- | --- |
| `rust-chatbot` | Desktop UI |
| `rcc` | Provider-aware launcher |
| `chatbot-worker` | Detached execution and spool updates |
| `chatbot-ctl` | Instance, worker, broker, and desktop-entry management |
| `chatbot-broker` | Relay between running instances |
| `chatbot-memory-mcp` | Workspace memory MCP service |
| `chatbot-session-mcp` | Session MCP service |
| `chatbot-history-auditor-mcp` | History-auditor MCP integration |

The adapter workspace separately builds `agent-adapter-cli`.

## Runtime Model

The app records user intent, writes a worker request, and launches a detached
worker. Worker events and results are persisted in the shared runtime spool.
Background tasks poll the spool and history data; the frame loop applies cached
updates, renders messages, and reconciles terminal results. The broker relays
updates between instances. See [worker-spool.md](worker-spool.md),
[broker-sync.md](broker-sync.md), and
[session-worker-lifecycle-spec.md](session-worker-lifecycle-spec.md).

## Event Queue and UI boundaries

UI actions and background deliveries converge through the session event queue.
Session selection and pending drafts must remain consistent while a response is
streaming. Passive refresh must not replace an active draft. File scans,
workspace normalization, history refresh, and worker polling use background
channels rather than blocking steady-state rendering. The contract and native
regression references are maintained in
[ui-lean-send-draft-stream-spec.md](ui-lean-send-draft-stream-spec.md).

The adapters in `src/ui/` translate application state into `chatbot-ui` traits.
The UI library does not own provider credentials, persistence, or process launch.

## Provider Support

The worker's `src/provider_catalog.rs` defines provider identities, aliases,
configuration paths, and transport families. `workflow-registry/providers.json`
defines the smaller autonomous policy subset. These are separate concerns:
a provider appearing in the UI does not imply it is enabled for autonomous work.
OmniForge model selection is session state, with app actions and the model
catalog adapter under `src/ui/`; provider process handling stays in the worker.

## Release Build Contract

Build `rust-chatbot`, `chatbot-worker`, the controller/launcher, and
`agent-adapter-cli` together. Helpers in `scripts/common.sh` manage the release
stack; supported overrides include `RUST_CHATBOT_ADAPTER_ROOT` and
`RUST_CHATBOT_ADAPTER_TARGET_DIR`. Build artifacts use release mode. Optional
UI/browser automation needs its own external tools and display environment.

## Storage and integrations

Runtime state defaults to `~/.rust-chatbot/`; older locations remain compatibility
fallbacks. Provider authentication is local runtime state; `.codex/auth.json`
is excluded from Git and from exported history. The workspace stores
provider sessions, spool records, configuration,
and memory state outside this source checkout. See
[chatbot-history-integration.md](chatbot-history-integration.md),
[memmachine-integration.md](memmachine-integration.md), and
[history-auditor-v1.md](history-auditor-v1.md) for integration contracts and limits.

## Verification

[../verification.md](../verification.md) lists the executable contracts,
focused regressions, and sibling checks. Architecture documents describe code
and intended invariants; they do not certify a completed test run.

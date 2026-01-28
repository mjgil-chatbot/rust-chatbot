# Broker And Sync

This document describes the current cross-instance synchronization model.

## Roles

| Component | Role |
|-----------|------|
| `chatbot-broker` | Unix socket relay daemon |
| `rust-chatbot` | UI client that sends and receives sync events |
| `chatbot-ctl` | Status and broker management CLI |

## Current Behavior

- The UI always starts a socket client.
- Connection happens asynchronously in a background thread.
- If the broker is unavailable, the UI keeps working locally.
- The client automatically reconnects with exponential backoff.
- Send operations are best-effort and are dropped while disconnected.
- Passive session-list refreshes are strictly sidebar-only. They update
  sidebar metadata via `SessionListPatch` and must never replace the active
  chat. Explicit current-session reloads (startup, user click, session
  creation, deletion fallback) use `SelectedSessionLoad` instead.

### Passive vs Explicit Refresh Routing

The UI distinguishes two refresh result types. `SessionListPatch` carries
sidebar metadata only and is used by passive SHM-triggered refreshes, socket
session-changed events, non-current-chat `StreamBegin`/`StreamEnd`/
`StreamStopped` notifications, and the argument-less `spawn_refresh_sessions_async()`.
Applying a `SessionListPatch` updates the `sessions` vector and cache but
never touches `current_session` and never calls `set_current_session()`.

`SelectedSessionLoad` carries both sidebar metadata and a full `ChatSession`.
It is used by explicit user selection via `spawn_load_session_async()`, session
creation via `spawn_create_session_async()`, deletion fallback, authoritative
reload, and startup. These paths intentionally replace the current session.

When `pending_draft` is active for the current chat (the user has submitted a
message and the assistant response has not yet started streaming), passive
refresh paths skip stale current-session replacement entirely. The draft state
signals an active send, so the in-memory session is authoritative until the
first stream chunk arrives or the turn terminates.

## Broker Lifecycle

The current implementation manages the broker through a `systemd --user`
service via `chatbot-ctl`.

Supported commands:

- `chatbot-ctl install-broker`
- `chatbot-ctl uninstall-broker`
- `chatbot-ctl start-broker`
- `chatbot-ctl stop-broker`
- `chatbot-ctl restart-broker`
- `chatbot-ctl broker-status`
- `chatbot-ctl broker-logs`

The broker itself stays intentionally simple:

- bind the shared-library broker socket path (`chatbot_lib::sync::broker_socket_path()`)
- write the shared-library broker PID path (`chatbot_lib::sync::broker_pid_path()`)
- accept client connections
- relay opaque messages to other clients
- remove dead clients on read or write failure

Broker path resolution is centralized in the library sync module. The UI socket
client, `chatbot-broker`, and `chatbot-ctl` all resolve the same socket and PID
paths through shared helpers so path changes cannot drift between binaries.

## Socket Protocol

Wire framing uses a little-endian length prefix:

```text
[4 bytes: u32 LE length][JSON payload]
```

Current message types:

```json
{"t":"begin",   "c":"<chat_id>", "p":"claude"}
{"t":"data",    "c":"<chat_id>", "p":"claude", "r":"partial response"}
{"t":"end",     "c":"<chat_id>", "p":"claude", "r":"final response", "sid":"<session_id>"}
{"t":"stopped", "c":"<chat_id>", "p":"claude", "r":"partial response"}
{"t":"feedback","c":"<chat_id>", "p":"claude", "r":"tool thinking", "is_error":false}
{"t":"sessions"}
```

The broker does not interpret these payloads. It only relays bytes.

Coverage now includes a socket-level integration test,
`socket_client_relays_sessions_changed_over_real_broker_socket`, which binds a
real Unix socket, lets `rust-chatbot` connect through the normal reader thread,
injects a framed `sessions` payload from the broker side, and verifies the UI
client also writes framed sync messages back through the connected socket.

## Failure Model

- If the broker dies, local streaming and recovery continue through the worker spool.
- Other windows stop receiving live updates until the broker is back.
- Once the broker returns, UI instances reconnect automatically.
- Session correctness does not depend on the broker; it depends on the durable spool.

## Related Docs

- [Overview](overview.md)
- [Worker Spool](worker-spool.md)

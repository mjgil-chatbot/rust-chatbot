# MemMachine Integration

This page describes the current MemMachine-backed workspace memory integration
in `rust-chatbot`.

It is implemented in the app runtime. It is not a provider-native feature of
the standalone `codex`, `claude`, or `gemini` CLIs.

## Current Behavior

The app uses workspace-scoped MemMachine projects to retrieve prior chat
snippets and inject them into provider prompts at send time.

The current flow is:

1. `send_message()` saves the raw user message as usual.
2. The app resolves the launched workspace and retrieves memory hits for that
   workspace from three layers: pinned manual memories, a local agent-memory
   packet store, and the remote MemMachine project.
3. The worker request carries an optional `MemoryContext`.
4. The worker forwards that context to the selected provider adapter.
5. The provider adapter prepends an app-managed memory preamble to the
   effective query.
6. After a worker reaches a terminal state, the app indexes finalized user and
   assistant messages back into the workspace MemMachine project and rebuilds
   the local agent-memory store from authoritative session JSON.

This path is shared by Codex, Claude, and Gemini when they are launched
through `rust-chatbot`.

## Storage Layout

Workspace memory still keeps app-owned local state under the shared infra root:

```text
<shared-infra>/
  memory/
    <workspace_hash>/
      ingest-state.json
      manual-memories.json
      agent-memory-store.json
      writer.lock
```

Notes:

- `<workspace_hash>` is derived from the canonical `launched_from` path.
- `ingest-state.json` stores app-owned indexing checkpoints and invalidation
  metadata.
- `manual-memories.json` stores explicitly remembered pinned facts for that
  workspace.
- `agent-memory-store.json` stores the app-owned `agentic_memory`
  `InMemoryAgentStore` rebuilt from authoritative chat sessions for that
  workspace.
- `writer.lock` serializes writes across multiple app windows.
- Chat transcript episodes themselves live in MemMachine, not in a local
  embedded index file.
- The MemMachine project id is the workspace hash, and the default org id is
  `rust-chatbot`.

## Backend Contract

The worker talks to a MemMachine server over HTTP. By default it targets:

- base URL: `http://127.0.0.1:8080`
- org id: `rust-chatbot`
- project id: `<workspace_hash>`

These values can be overridden in `[memory]` config with:

- `memmachine_base_url`
- `memmachine_org_id`

The worker creates the workspace project lazily on first index/write. Retrieval
checks whether the project exists before searching it.

Read-oriented MemMachine calls such as project probes and retrieval keep a
short HTTP timeout so prompt-time memory lookups fail open quickly. Indexing
writes use a separate longer timeout because large workspace rebuilds can take
minutes while MemMachine batches embeddings and persistence.

Some MemMachine deployments may sit behind a provider-translation proxy rather
than calling a model vendor directly. This is relevant for MiniMax-backed
deployments: MemMachine's open-source config supports OpenAI-style embedders,
while MiniMax's embeddings API uses a different request and response shape.
Using a small local proxy preserves the existing MemMachine contract for
`rust-chatbot` while allowing the backend deployment to swap providers.

## Indexed Record Shape

The current integration indexes individual chat messages, not full session
dumps.

Each record keeps the same deterministic `doc_key`:

```text
<chat_id>:<message_index>:<role>
```

The indexed metadata includes:

- `chat_id`
- `provider`
- `role`
- `message_index`
- `turn_index`
- `timestamp`
- `workspace_path`
- `session_name`
- `final_state` when present

This keeps retrieval granular and makes invalidation practical without storing
whole transcripts in the prompt.

## Retrieval Path

Retrieval happens before the worker request is written.

The app:

1. loads local invalidations and pinned manual memories
2. loads the local `agent-memory-store.json` and builds a small
   `PromptMemoryPacket` for the current query
3. searches the workspace MemMachine project for episodic hits
4. maps packet items and matching episodes into `MemoryHit` records
5. filters invalidated `doc_key`s, then deduplicates by `doc_key` so the
   higher-priority local hit wins when it overlaps a remote MemMachine record
6. attaches the resulting `MemoryContext` to the worker request

Retrieval deliberately skips trivial shell-style prompts such as `ls`, `pwd`,
and `git status`. Those requests should execute against the current workspace
state directly, and pulling prior chat memory into them creates prompt noise
without improving accuracy.

Provider adapters format the injected prompt as:

```text
Use the retrieved chat memory only when it is relevant and do not treat it as
authoritative if it conflicts with the current workspace state.

Retrieved memory:
- [session=<...> role=<...> time=<...>] <snippet>
- ...

User request:
<raw user query>
```

The UI also persists that retrieved-memory preview onto the user turn metadata.
When the following assistant message is rendered, the message panel shows a
collapsed `Retrieved memory (...)` section directly above the existing `Tool
calls and intermediate responses (...)` section so the operator can see the
exact memory lines that were injected for that request.

## Indexing Path

MemMachine writes happen outside streaming.

The current indexing behavior is:

- Startup backfill groups existing sessions by workspace and indexes them.
- Terminal worker reconciliation re-indexes the relevant workspace after
  `done`, `stopped`, and `error` outcomes.
- Writes are serialized with `writer.lock`.
- A bounded retry is used for write lock contention.
- Session JSON remains authoritative; MemMachine can be rebuilt from it.
- The local agent-memory store is rebuilt from that same authoritative session
  JSON on each indexing pass instead of maintaining a separate incremental log.
- Worker command strings are not indexed into memory metadata.
- Assistant messages that begin with the standard bracket-timestamped progress
  transcript and end with a real answer body are indexed from that final body
  only.
- Assistant messages that contain only that final durable transcript are still
  indexed as-is. This preserves recurring tool-misuse failures, such as a
  model repeatedly omitting a required tool field, so retrieval can surface
  them back into future prompts.

The exact retrieved-memory preview injected into a user turn is also persisted
onto that user message's metadata in session JSON. The `chatbot-memory-mcp`
surface can list those persisted injections separately from the indexed memory
corpus so operators can inspect what was actually sent back through the prompt
pipeline.

In addition, each send attempt now appends a workspace-scoped retrieval audit
entry under the shared memory root. Those entries persist the query, candidate
hits returned by retrieval, a candidate preview, and the injected preview that
was actually attached to the outgoing prompt. This makes it possible to inspect
both successful injections and zero-hit retrieval attempts after the fact.
Provider-neutral `/memory-retrievals` wrappers call the MCP surface for this
audit so operators can inspect those candidate records without manual MCP
tooling.

`ingest-state.json` tracks per-session indexing progress and fingerprints so
the app can append when safe and rebuild when history diverges.

Rebuild behavior now resets the workspace MemMachine project and replays the
current session JSON back into it.

## Explicit Memory

The app also supports explicit pinned workspace facts outside normal chat
history.

Current behavior:

- Explicit facts are stored in `manual-memories.json`.
- Retrieval scores them separately and inserts matching pinned facts ahead of
  normal MemMachine chat hits.
- High-priority explicit facts are intended for stable workspace facts the user
  wants remembered directly.
- The `chatbot-memory-mcp` server exposes `remember_fact`, `forget_fact`, and
  `list_manual_memories`.

## Invalidation Layer

Invalidations are stored in `ingest-state.json` under `invalidated_doc_keys`,
keyed by `doc_key`, with:

- `invalidated_at`
- optional `reason`

Current behavior:

- `invalidate_memory_doc_key(...)` marks a record as invalid.
- `restore_memory_doc_key(...)` removes that invalidation entry.
- Retrieval suppresses any hit whose `doc_key` appears in
  `invalidated_doc_keys`.
- The underlying MemMachine episode is not mutated by this layer.

This is intentional. The app treats session JSON plus local invalidation state
as authoritative and does not need direct per-episode mutation support from the
remote memory backend to hide stale hits.

## Failure Model

MemMachine must stay fail-open.

Current rules:

- Retrieval errors fall back to whichever local layers are still available.
- Agent-memory load or packet-build failures do not block MemMachine retrieval.
- Indexing errors do not block normal chat completion.
- Status inspection still reports local ingest/manual-memory state even if the
  MemMachine server is down.
- Session JSON stays authoritative even if MemMachine is stale or unavailable.

## Current Native Enforcement

The current repo-local enforcement surface for the memory contract is:

- `scripts/verify_memory_contract.py`
- `scripts/verify-memory-regressions.sh`

The verifier keeps this memory integration document, the verification mapping,
and the focused release regression bundle aligned. The release bundle exercises
workspace memory status/search routing, persisted retrieved-memory previews,
retrieval audit candidate-plus-injected preview inspection, invalidation and
restore flows, explicit-memory validation, and workspace-boundary enforcement.

## Relevant Code

The main implementation lives in:

- `../chatbot-worker/src/memory/mod.rs`
- `../chatbot-worker/src/memory/backend.rs`

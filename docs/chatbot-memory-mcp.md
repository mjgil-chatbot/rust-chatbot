# Memory MCP

`chatbot-memory-mcp` is a small stdio MCP server that exposes the app-owned
workspace memory operations to agents.

It is intentionally narrow. The tools operate on the same `doc_key`-based
memory layer used by `rust-chatbot`; they do not expose direct remote episode
mutation inside MemMachine.

## Tools

- `get_workspace_memory_status`
- `search_memory`
- `list_memory_injections`
- `list_memory_retrievals`
- `remember_fact`
- `forget_fact`
- `list_manual_memories`
- `list_invalidations`
- `invalidate_doc_key`
- `restore_doc_key`

## Configuration

Environment variables:

| Variable | Default | Description |
|---|---|---|
| `CHATBOT_MEMORY_MCP_ALLOWED_ROOTS` | current working directory | Path-separated list of absolute workspace roots the server may access |

If you want the server to work across repos in `/home/example/git`, set:

```bash
export CHATBOT_MEMORY_MCP_ALLOWED_ROOTS=/home/example/git
```

## Usage

Example MCP server definition:

```json
{
  "mcpServers": {
    "chatbot-memory": {
      "command": "/absolute/path/to/chatbot-memory-mcp",
      "env": {
        "CHATBOT_MEMORY_MCP_ALLOWED_ROOTS": "/home/example/git"
      }
    }
  }
}
```

Example invalidation call:

```json
{
  "workspace_path": "/workspace/rust-chatbot",
  "doc_key": "1650a61e-20c9-46b1-bfbc-554d5a0dec6a:3:assistant",
  "reason": "stale design note"
}
```

Example explicit remember call:

```json
{
  "workspace_path": "/workspace/rust-chatbot",
  "fact": "The staging API base URL is https://staging.internal.example.",
  "priority": "high"
}
```

Example recent-injections call:

```json
{
  "workspace_path": "/workspace/rust-chatbot",
  "limit": 10
}
```

Example recent-retrievals call:

```json
{
  "workspace_path": "/workspace/rust-chatbot",
  "limit": 10
}
```

## User Commands

The repo also exposes provider-neutral workflow commands for these memory
operations:

- `/memory-remember`
- `/memory-list`
- `/memory-forget`
- `/memory-search`
- `/memory-status`
- `/memory-retrievals`
- `/memory-invalidate`
- `/memory-invalidations`
- `/memory-restore`

To publish those memory wrappers into home-level Claude/Codex command
locations so they are available across workspaces, run:

```bash
python3 scripts/generate_provider_workflows.py --install-home-memory
```

## Notes

- This server uses the existing backend helpers in
  `../chatbot-worker/src/memory/mod.rs`.
- Explicit remembered facts are stored as app-owned pinned memory and are
  surfaced ahead of regular chat history when they match the query.
- Retrieved-memory previews injected into user turns are persisted on the user
  message metadata inside session JSON, and `list_memory_injections` reads
  those stored previews back out in timestamp order.
- Candidate retrievals are also persisted under the workspace memory root in
  `retrieval-audit.json`, and `list_memory_retrievals` exposes the candidate
  hits, candidate preview, and injected preview for each recent send attempt.
- Invalidations hide hits from future retrieval; they do not mutate the
  underlying MemMachine project directly.
- The workspace path must be absolute and resolve under an allowed root.

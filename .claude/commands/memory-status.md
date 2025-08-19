# Memory Status

Use this command to inspect the workspace memory store, including indexing, invalidations, and pinned manual facts.

Invocation:

```text
/memory-status
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants a quick health check of workspace memory
- When debugging whether memory exists for the current repo

## Inputs

- No structured inputs beyond the command arguments.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `get_workspace_memory_status` tool.
3. Summarize the current memory file, ingest checkpoint, invalidation count, and manual memory count.

## Expected outputs

- Workspace memory status
- Counts for indexed sessions, invalidations, and manual memories

## Examples

```text
/memory-status
```

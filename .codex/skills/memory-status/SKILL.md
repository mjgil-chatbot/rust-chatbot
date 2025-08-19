---
name: "memory-status"
description: "Use when the user wants to inspect the workspace memory store, including indexing, invalidations, and pinned manual facts."
---

# Memory Status

Use this skill when the user wants to inspect the workspace memory store, including indexing, invalidations, and pinned manual facts.

## When to use

- When the user wants a quick health check of workspace memory
- When debugging whether memory exists for the current repo

## Inputs

- No structured inputs beyond the command arguments.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `get_workspace_memory_status` tool.
3. Summarize the current memory file, ingest checkpoint, invalidation count, and manual memory count.

## Output contract

- Workspace memory status
- Counts for indexed sessions, invalidations, and manual memories

## Example invocation

```text
/memory-status
```

---
name: "memory-invalidations"
description: "Use when the user wants to list doc_keys currently invalidated for the current workspace."
---

# Memory Invalidations

Use this skill when the user wants to list doc_keys currently invalidated for the current workspace.

## When to use

- When the user wants to inspect what memories are currently suppressed
- When choosing a doc_key to restore

## Inputs

- No structured inputs beyond the command arguments.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `list_invalidations` tool.
3. Present the invalidated doc_keys and reasons.

## Output contract

- Invalidated doc_key list
- Stored reasons
- Workspace invalidation count

## Example invocation

```text
/memory-invalidations
```

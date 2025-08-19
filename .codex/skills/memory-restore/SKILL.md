---
name: "memory-restore"
description: "Use when the user wants to restore a previously invalidated memory doc_key so retrieval can return it again."
---

# Memory Restore

Use this skill when the user wants to restore a previously invalidated memory doc_key so retrieval can return it again.

## When to use

- When the user wants to undo a prior invalidation
- When a suppressed memory turns out to still be useful

## Inputs

- `doc_key` (required): Deterministic document key to restore.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `restore_doc_key` tool.
3. Confirm whether the invalidation was removed.

## Output contract

- Restore confirmation
- Whether any change occurred

## Example invocation

```text
/memory-restore 1650a61e-20c9-46b1-bfbc-554d5a0dec6a:3:assistant
```

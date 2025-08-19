---
name: "memory-forget"
description: "Use when the user wants to remove a pinned manual memory fact from the current workspace."
---

# Memory Forget

Use this skill when the user wants to remove a pinned manual memory fact from the current workspace.

## When to use

- When the user wants to remove an explicitly remembered fact
- When a pinned fact is stale or no longer useful

## Inputs

- `memory_id` (required): Manual memory identifier returned by /memory-list or /memory-remember.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `forget_fact` tool.
3. Confirm whether the pinned memory was removed.

## Output contract

- Forget confirmation
- Whether any change occurred

## Example invocation

```text
/memory-forget 5aaf57fb-1f87-4d08-bab4-8af7c620c7a4
```

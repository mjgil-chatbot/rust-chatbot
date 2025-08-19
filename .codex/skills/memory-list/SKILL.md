---
name: "memory-list"
description: "Use when the user wants to list pinned manual memories for the current workspace."
---

# Memory List

Use this skill when the user wants to list pinned manual memories for the current workspace.

## When to use

- When the user wants to inspect explicitly remembered facts
- When choosing a memory_id to forget later

## Inputs

- No structured inputs beyond the command arguments.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `list_manual_memories` tool.
3. Present the memories with their ids and priorities.

## Output contract

- Pinned memory list
- memory_id values
- Priority and fact text

## Example invocation

```text
/memory-list
```

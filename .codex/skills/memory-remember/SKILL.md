---
name: "memory-remember"
description: "Use when the user wants to persist an explicit workspace fact at high priority so future retrieval can surface it ahead of normal chat history."
---

# Memory Remember

Use this skill when the user wants to persist an explicit workspace fact at high priority so future retrieval can surface it ahead of normal chat history.

## When to use

- When the user wants the app to remember a stable fact for this workspace
- When a fact should be pinned even if it was not part of a chat turn

## Inputs

- `fact` (required): Fact text to persist as pinned workspace memory.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path the same way the chat session does.
2. Call the `chatbot-memory` MCP server `remember_fact` tool with priority `high`.
3. Return the resulting `memory_id` and `doc_key` so the user can manage it later.

## Output contract

- Pinned memory confirmation
- Returned memory_id
- Returned doc_key

## Example invocation

```text
/memory-remember The staging API base URL is https://staging.internal.example.
```

---
name: "memory-search"
description: "Use when the user wants to search workspace memory exactly the way the app does before prompt injection."
---

# Memory Search

Use this skill when the user wants to search workspace memory exactly the way the app does before prompt injection.

## When to use

- When the user wants to inspect what memory retrieval would return
- When debugging whether a pinned fact or prior chat would be found

## Inputs

- `query` (required): Search query to run through the memory retrieval path.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `search_memory` tool.
3. Show the retrieved hits in priority order.

## Output contract

- Memory search results
- hit_count
- Returned hits and scores

## Example invocation

```text
/memory-search staging API base URL
```

# Memory Search

Use this command to search workspace memory exactly the way the app does before prompt injection.

Invocation:

```text
/memory-search <query>
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants to inspect what memory retrieval would return
- When debugging whether a pinned fact or prior chat would be found

## Inputs

- `query` (required): Search query to run through the memory retrieval path.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `search_memory` tool.
3. Show the retrieved hits in priority order.

## Expected outputs

- Memory search results
- hit_count
- Returned hits and scores

## Examples

```text
/memory-search staging API base URL
```

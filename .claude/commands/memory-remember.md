# Memory Remember

Use this command to persist an explicit workspace fact at high priority so future retrieval can surface it ahead of normal chat history.

Invocation:

```text
/memory-remember <fact>
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants the app to remember a stable fact for this workspace
- When a fact should be pinned even if it was not part of a chat turn

## Inputs

- `fact` (required): Fact text to persist as pinned workspace memory.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path the same way the chat session does.
2. Call the `chatbot-memory` MCP server `remember_fact` tool with priority `high`.
3. Return the resulting `memory_id` and `doc_key` so the user can manage it later.

## Expected outputs

- Pinned memory confirmation
- Returned memory_id
- Returned doc_key

## Examples

```text
/memory-remember The staging API base URL is https://staging.internal.example.
```

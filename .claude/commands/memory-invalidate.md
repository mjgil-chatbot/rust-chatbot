# Memory Invalidate

Use this command to hide a memory hit from future retrieval by invalidating its doc_key.

Invocation:

```text
/memory-invalidate <doc_key> [reason]
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When a retrieved chat memory is wrong, stale, or misleading
- When the user wants to suppress a specific memory without deleting all chat history

## Inputs

- `doc_key` (required): Deterministic document key like <chat_id>:<message_index>:<role>.
- `reason` (optional): Optional reason to store alongside the invalidation.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `invalidate_doc_key` tool.
3. Confirm the invalidation result and show the reason if one was stored.

## Expected outputs

- Invalidation confirmation
- Whether the doc_key changed state

## Examples

```text
/memory-invalidate 1650a61e-20c9-46b1-bfbc-554d5a0dec6a:3:assistant stale design note
```

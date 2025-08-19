# Memory Restore

Use this command to restore a previously invalidated memory doc_key so retrieval can return it again.

Invocation:

```text
/memory-restore <doc_key>
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants to undo a prior invalidation
- When a suppressed memory turns out to still be useful

## Inputs

- `doc_key` (required): Deterministic document key to restore.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `restore_doc_key` tool.
3. Confirm whether the invalidation was removed.

## Expected outputs

- Restore confirmation
- Whether any change occurred

## Examples

```text
/memory-restore 1650a61e-20c9-46b1-bfbc-554d5a0dec6a:3:assistant
```

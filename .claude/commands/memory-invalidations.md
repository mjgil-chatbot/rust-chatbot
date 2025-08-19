# Memory Invalidations

Use this command to list doc_keys currently invalidated for the current workspace.

Invocation:

```text
/memory-invalidations
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants to inspect what memories are currently suppressed
- When choosing a doc_key to restore

## Inputs

- No structured inputs beyond the command arguments.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `list_invalidations` tool.
3. Present the invalidated doc_keys and reasons.

## Expected outputs

- Invalidated doc_key list
- Stored reasons
- Workspace invalidation count

## Examples

```text
/memory-invalidations
```

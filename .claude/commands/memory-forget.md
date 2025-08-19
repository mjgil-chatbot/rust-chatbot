# Memory Forget

Use this command to remove a pinned manual memory fact from the current workspace.

Invocation:

```text
/memory-forget <memory_id>
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants to remove an explicitly remembered fact
- When a pinned fact is stale or no longer useful

## Inputs

- `memory_id` (required): Manual memory identifier returned by /memory-list or /memory-remember.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `forget_fact` tool.
3. Confirm whether the pinned memory was removed.

## Expected outputs

- Forget confirmation
- Whether any change occurred

## Examples

```text
/memory-forget 5aaf57fb-1f87-4d08-bab4-8af7c620c7a4
```

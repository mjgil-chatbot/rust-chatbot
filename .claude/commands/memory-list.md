# Memory List

Use this command to list pinned manual memories for the current workspace.

Invocation:

```text
/memory-list
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants to inspect explicitly remembered facts
- When choosing a memory_id to forget later

## Inputs

- No structured inputs beyond the command arguments.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Call the `chatbot-memory` MCP server `list_manual_memories` tool.
3. Present the memories with their ids and priorities.

## Expected outputs

- Pinned memory list
- memory_id values
- Priority and fact text

## Examples

```text
/memory-list
```

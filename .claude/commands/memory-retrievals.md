# Memory Retrievals

Use this command to inspect recent workspace memory retrieval attempts, including candidate hits and the injected preview that was actually attached to the prompt.

Invocation:

```text
/memory-retrievals [limit]
```

Arguments from the user are provided in `$ARGUMENTS`.

## When to use

- When the user wants to inspect which memory candidates were considered for recent sends
- When debugging why a retrieved-memory preview was or was not injected
- When auditing zero-hit retrieval attempts

## Inputs

- `limit` (optional): Maximum number of recent retrieval attempts to return. Defaults to 20.

## Context to load

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Workflow

1. Resolve the current workspace path.
2. Parse `$ARGUMENTS` as an optional integer limit.
3. Call the `chatbot-memory` MCP server `list_memory_retrievals` tool.
4. Show the retrieval audit path, candidate hit count, candidate preview, and injected preview for each returned entry.

## Expected outputs

- Memory retrieval audit summary
- retrieval_count
- retrieval_audit_path
- Returned retrieval entries with candidate hits and injected previews

## Examples

```text
/memory-retrievals
```

```text
/memory-retrievals 10
```

---
name: "memory-retrievals"
description: "Use when the user wants to inspect recent workspace memory retrieval attempts, including candidate hits and injected previews."
---

# Memory Retrievals

Use this skill when the user wants to inspect recent workspace memory retrieval attempts, including candidate hits and the injected preview that was actually attached to the prompt.

## When to use

- When the user wants to inspect which memory candidates were considered for recent sends
- When debugging why a retrieved-memory preview was or was not injected
- When auditing zero-hit retrieval attempts

## Inputs

- `limit` (optional): Maximum number of recent retrieval attempts to return. Defaults to 20.

## Required context

- docs/chatbot-memory-mcp.md
- docs/architecture/memmachine-integration.md

## Steps

1. Resolve the current workspace path.
2. Parse the user arguments as an optional integer limit.
3. Call the `chatbot-memory` MCP server `list_memory_retrievals` tool.
4. Show the retrieval audit path, candidate hit count, candidate preview, and injected preview for each returned entry.

## Output contract

- Memory retrieval audit summary
- retrieval_count
- retrieval_audit_path
- Returned retrieval entries with candidate hits and injected previews

## Example invocation

```text
/memory-retrievals
```

```text
/memory-retrievals 10
```

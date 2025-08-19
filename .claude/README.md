# Claude Desktop Chatbot

This directory contains configuration files for the Claude Desktop Chatbot.

## Custom Commands

You can create custom slash commands by adding markdown files to the `commands/` subdirectory.

Example: Create `commands/explain.md`:

```markdown
# Explain Code

Please explain the following code in detail, including:
- What it does
- How it works
- Any potential improvements
- Best practices it follows or violates

Code to explain:
$ARGUMENTS
```

Then you can use `/explain` in the desktop chatbot to trigger this custom command.

These command files are generated from the provider-neutral workflow registry in
`workflow-registry/commands/*.json` via:

```bash
python3 scripts/generate_provider_workflows.py
```

The goal is to keep Claude and Codex workflow definitions in sync from one
source of truth.

## Configuration

The desktop chatbot uses the same configuration as the Claude CLI. You can configure global settings using:

```bash
claude config set model sonnet
claude config set -g theme dark
```

For more configuration options, see: https://docs.anthropic.com/en/docs/claude-code/cli-reference

## Local configuration

Provider credentials, session records, and MCP configuration are local runtime
state. They are deliberately excluded from source control. The optional
`mcp.example.json` at the repository root illustrates a portable setup; configure
only the services and allowed workspace roots that you need.

# Input Completion

The chat input field supports tab completion for slash commands and
arrow-key navigation through message history.

## Slash Command Completion (Tab)

When the input starts with `/`, pressing **Tab** autocompletes the command:

- **Single match** — completes the full command and appends a trailing space.
- **Multiple matches** — completes to the longest common prefix. If the
  prefix doesn't narrow things down, the highest-priority match wins
  (project/personal skills take priority over built-in commands).

### Discovery Order

Commands are discovered at startup in priority order:

1. **Project provider commands** — for the active workspace, Claude-family
   providers read `<cwd>/.claude/commands/*.md` and
   `<cwd>/.claude/skills/<name>/SKILL.md`; Codex-family providers read
   `<cwd>/.codex/skills/<name>/SKILL.md`
2. **Personal provider commands** — the same provider-specific locations under
   `HOME`
3. **Provider extra commands** — today this is `/batch`, `/claude-api`,
   `/debug`, `/loop`, and `/simplify` for Claude, MiniMax, and Qwen
4. **Built-in commands** — the hardcoded slash commands (`/help`, `/clear`,
   `/model`, etc.) for Claude and MiniMax

Gemini and OpenRouter do not currently contribute project or personal slash
commands. MiniMax and Qwen follow the Claude-family discovery paths. Nvidia
follows the Codex-family discovery paths.

## Slash Command Cycling (Up/Down)

While the input starts with `/`, **Up** and **Down** cycle through matching
commands instead of browsing message history:

- The typed prefix is stashed so you can cycle back to it.
- Wraps around at both ends of the match list.
- Exiting slash mode (clearing the `/`) restores normal arrow-key behavior.

## Message History (Up/Down)

When the input does *not* start with `/`:

- **Up** — walks backward through user messages in the current session.
- **Down** — walks forward; going past the newest message restores whatever
  you were typing before you started browsing.

## Implementation

The input widget integration lives in `src/app/ui/main_panel.rs`. The TextEdit
uses `.lock_focus(true)` so Tab triggers completion instead of moving focus to
the next widget. Command discovery is implemented in
`src/app/state/helpers_impl.rs` (`discover_slash_commands`).

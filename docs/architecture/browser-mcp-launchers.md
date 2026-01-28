# Browser MCP Launchers

## Overview

Browser-facing MCP integrations in this repo are launched through repo-local
wrappers instead of ad hoc npm installs.

## Chrome DevTools Paths

- `browser-mcp-cgroups/chrome-devtools.sh` is the primary Chrome DevTools MCP
  launcher for normal repo use. It runs the local
  `mcp-servers/chrome-devtools-mcp` binary under the transient cgroup wrapper
  and falls back to `cargo run --release` against the local stdio-server
  manifest when no built binary is present.
- `chrome-devtools-mcp-2/launch.sh` is the Obscura-backed variant. It ensures a
  CDP endpoint is available on `127.0.0.1`, then launches the same local
  `mcp-servers/chrome-devtools-mcp` implementation with `--browser-url`
  instead of using `npx chrome-devtools-mcp@latest`.

## Operational Note

The local-repo launch contract keeps Codex and other provider configs aligned
on one Chrome DevTools MCP implementation and avoids repopulating npm cache
copies under `~/.npm/_npx` during normal launcher use.

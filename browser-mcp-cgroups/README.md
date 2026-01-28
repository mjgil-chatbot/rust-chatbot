# browser-mcp-cgroups

Launch wrappers for browser-related MCP servers under a transient user service with a
default memory limit of `8G`.

## Servers

- `chrome-devtools.sh`
- `puppeteer.sh`

Both wrappers call `cgroup-wrap.sh`, which uses `systemd-run --user`
with `MemoryMax=${BROWSER_MCP_MEMORY_MAX:-8G}` and
`LimitAS=${BROWSER_MCP_LIMIT_AS:-infinity}`, then falls back to direct exec if
the transient service cannot be created.

## Environment

- `BROWSER_MCP_MEMORY_MAX`: memory ceiling for the service. Default: `8G`
- `BROWSER_MCP_LIMIT_AS`: address-space limit for the service. Default: `infinity`

## Notes

- `chrome-devtools.sh` prefers the local release binary and falls back to
  `cargo run --release`. When available, it also prefers the real Chrome binary
  at `/opt/google/chrome/chrome` over the shell wrapper in `/usr/bin`.
- `puppeteer.sh` prefers the local release binary and falls back to
  `cargo run --release`. When available, it also prefers the real Chrome binary
  at `/opt/google/chrome/chrome` over the shell wrapper in `/usr/bin`.

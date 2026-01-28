# chrome-devtools-mcp-2

Standalone wrapper that runs `chrome-devtools-mcp` against an Obscura CDP
endpoint instead of launching Chrome.

## What it does

1. Starts `obscura serve` on a local port if no CDP server is already present.
2. Waits for the CDP endpoint to become available on `127.0.0.1`.
3. Launches `chrome-devtools-mcp` with `--browser-url`.

This keeps the integration isolated from the existing `chrome-devtools` MCP
server config.

## Files

- `launch.sh`: starts Obscura if needed and then execs `chrome-devtools-mcp`.
- `.gitignore`: ignores local runtime logs and PID files.

## Requirements

- A local `chrome-devtools-mcp` checkout or built binary
- Obscura binary available via one of:
  - `OBSCURA_BIN=/path/to/obscura`
  - `command -v obscura`
  - `/workspace/others/obscura/target/release/obscura`

## MCP config snippet

```json
{
  "mcpServers": {
    "chrome-devtools-mcp-2": {
      "command": "/workspace/rust-chatbot/chrome-devtools-mcp-2/launch.sh",
      "args": []
    }
  }
}
```

## Environment variables

- `OBSCURA_BIN`: explicit Obscura binary path.
- `OBSCURA_PORT`: CDP port. Default: `9222`.
- `OBSCURA_START_TIMEOUT_SECS`: startup wait timeout. Default: `20`.
- `OBSCURA_LOG_FILE`: log file path. Default: `./obscura.log` in this folder.
- `OBSCURA_EXTRA_ARGS`: extra args appended to `obscura serve`.
- `LOCAL_CHROME_DEVTOOLS_MCP_BIN`: optional override for the local built binary.
- `LOCAL_CHROME_DEVTOOLS_MCP_MANIFEST`: optional override for the local Cargo manifest.

## Manual test

```bash
./launch.sh
```

If you want to test Obscura separately first:

```bash
obscura serve --port 9222
curl http://127.0.0.1:9222/json/version
```

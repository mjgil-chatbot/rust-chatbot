# chatbot-ctl

Management CLI for rust-chatbot instances and the broker daemon.

Reads shared memory for live UI instances and the durable worker spool for job
execution state.

## Commands

### Instance management

| Command | Description |
|---------|-------------|
| `launch --provider <p> [--instance N] [--cwd DIR]` (or `--provider=<p>`) | Launch one UI instance for any supported provider alias (`claude`, `minimax`, `minimaxforge`, `qwen`, `codex`, `nvidia`, `gemini`, `geminiforge`, `openrouter`) |
| `restart-running` | Restart each currently running UI instance with its saved cwd and CLI args |
| `status` | Show broker, instances, and worker job state |
| `stop [PID...]` | Stop UI instances (all if no PIDs given); broker keeps running |
| `pids` | Print live instance PIDs, one per line (for scripting) |
| `stop-all` | Stop workers, UI instances, and broker |

### Worker spool

| Command | Description |
|---------|-------------|
| `workers` | List active worker spool jobs |
| `stop-workers [chat-id]` | Send `SIGTERM` to active workers (optionally scoped to one chat) |
| `gc` | Remove stale terminal jobs, dead-running jobs, and stale request-only spool entries whose `state.json` never appeared after the five-second startup grace period |

### Broker (systemd user service)

| Command | Description |
|---------|-------------|
| `install-broker` | Write systemd user unit and enable it |
| `uninstall-broker` | Disable and remove the systemd unit |
| `start-broker` | Start broker via systemd |
| `stop-broker` | Stop broker via systemd |
| `restart-broker` | Restart broker via systemd |
| `broker-status` | Show broker systemd status |
| `broker-logs` | Follow broker journal logs |

### Desktop integration

| Command | Description |
|---------|-------------|
| `install-desktop` | Write `.desktop` entries for each instance |
| `uninstall-desktop` | Remove `.desktop` entries |

Desktop entries are written to `~/.local/share/applications/` with names like
`0N-rust-chatbot.desktop` (one per provider instance). Each entry includes the
full path to the `chatbot-ctl` binary used at install time, so entries generated
from a debug build will point to `target/debug/chatbot-ctl` and entries from a
release build to `target/release/chatbot-ctl`.

Provider icons are PNG files in `assets/`:

| Provider | Icon file |
|----------|-----------|
| Claude | `assets/anthropic_logo.png` |
| MiniMax | `assets/minimax_logo.png` |
| MiniMaxForge | `assets/minimax_logo.png` |
| Qwen | `assets/qwen_logo.png` |
| Codex | `assets/openai_logo.png` |
| Gemini | `assets/gemini_logo.png` |
| GeminiForge | `assets/gemini_logo.png` |

#### Restoring or refreshing desktop entries

After a release build, re-run `install-desktop` to update the binary paths and
icon references:

```bash
cargo build --release
./target/release/chatbot-ctl install-desktop
update-desktop-database ~/.local/share/applications/
```

To remove stale entries (e.g. from a renamed or old project):

```bash
./target/release/chatbot-ctl uninstall-desktop
rm ~/.local/share/applications/codex-desktop-chat.desktop   # if present
./target/release/chatbot-ctl install-desktop
update-desktop-database ~/.local/share/applications/
```

#### Adding a new provider icon

1. Place the PNG in `assets/` (128x128 RGBA recommended).
2. Add a row to the `entries` array in `src/bin/chatbot_ctl/desktop.rs`
   `cmd_install_desktop()` with the instance number, provider, and icon filename.
3. Add a matching arm in `icon_data()` in `src/main.rs` so the window icon
   matches the `.desktop` entry.

## Helper scripts

| Script | Description |
|--------|-------------|
| `run_one.sh <provider> [instance]` | Build and launch one provider via `chatbot-ctl launch` |
| `start-all.sh` | Launch the default profile: Claude, Codex, Gemini |
| `stop-all.sh` | Stop all UI instances |
| `restart-all.sh` | Rebuild and restart the currently running instances, or start the default profile. If the live app is running from `~/.local/bin`, it refreshes that install before restarting. |

These helper scripts resolve `Cargo.toml` and built binaries from the repo
root rather than from `scripts/`, so they work when launched via `just` or by
running the script paths directly.

Release helper builds now include `chatbot-adapter`'s `agent-adapter-cli`, and
install sync copies that binary into the same `bin/` root as
`rust-chatbot`, `chatbot-worker`, `chatbot-ctl`, and `rcc`. If the adapter
checkout lives outside the default sibling path, set
`RUST_CHATBOT_ADAPTER_ROOT` before running the helper scripts or `just install`.
If the adapter needs a separate build cache location, set
`RUST_CHATBOT_ADAPTER_TARGET_DIR` as well.

By default, the helper scripts now build release binaries into
`${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot/target/ui-stack` and
`${XDG_CACHE_HOME:-$HOME/.cache}/rust-chatbot/target/agent-adapter` instead of
the repo-local `target/` tree. This avoids stale permission problems in shared
checkouts while preserving `CARGO_TARGET_DIR` and
`RUST_CHATBOT_ADAPTER_TARGET_DIR` as explicit overrides.

### How `restart-running` preserves directories

On Linux, `chatbot-ctl restart-running` reads `/proc/PID/cwd` and
`/proc/PID/cmdline` for each live instance, stops those instances, then relaunches
them from the same working directory with the same CLI args. `restart-all.sh`
wraps that command after rebuilding. If no instances were running, the script
falls back to `start-all.sh`.

### Launch-time backup scheduler preflight

Before `chatbot-ctl launch` (and any relaunch path that funnels through the same
spawn helper) starts the UI, it refreshes the per-user backup scheduler for the
current platform:

- Linux: installs or updates `rust-chatbot-backup.service` and
  `rust-chatbot-backup.timer` under `~/.config/systemd/user/`, with
  `Persistent=true` so missed daily `01:15` runs catch up after downtime. The
  service also uses `Restart=on-failure` with a short delay so abnormal exits
  are restarted immediately while the user manager stays alive.
- macOS: installs or updates `~/Library/LaunchAgents/com.rustchatbot.backup.plist`
  with `RunAtLoad` and `StartCalendarInterval` at `01:15`.

The managed backup runner retries failed backup executions up to three times
before reporting failure and records the last successful daily slot only after
a successful run. Archive compression uses `pigz` with the detected processor
count when available and falls back to standard `tar`/gzip otherwise. Before
capturing archives, the backup script terminates active stdio MCP server
processes and waits briefly for their hosts to respawn them, so macOS also
catches up after login/load when the machine was off at the scheduled time.
Linux launch-time migration also removes the old managed cron backup block so
the backup job does not run twice.

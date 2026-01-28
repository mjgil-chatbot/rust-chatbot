# Background Provider Runs

Background multi-agent work in the current tree runs through the in-process
dispatcher in `../chatbot-worker/src/worker_ext/background/`, not through the
deleted `src/coordination` engine.

## Execution Flow

1. The caller constructs `BackgroundTask` values with provider, prompt,
   optional system prompt, working directory, and resume session ID.
2. `BackgroundDispatcher` fans those tasks out with a bounded Tokio semaphore.
3. Each task enters `execute_provider_task`, which resolves the provider and
   applies `background_provider_overrides`.
4. `build_provider_command` builds the provider command invocation, syncs
   provider MCP config, and optionally wraps the command with `runsc`.
   For Claude-compatible providers, MCP sync now writes to all provider
   Claude-compatible JSON targets, including MiniMax's `~/.minimax/.claude.json`
   compatibility path, so resumed sessions keep MCP servers in scope for all
   supported transports. MiniMax command construction also threads the active
   session workspace into the shared Claude-compatible command builder, so
   workspace `CLAUDE.md` instructions appear in the same built command preview
   and failure-command metadata that the live CLI invocation uses.
5. For Codex-compatible providers, `sync_mcp_configs(...)` now writes MCP
   blocks to both `~/.rust-chatbot/.codex/config.toml` and legacy
   `~/.codex/config.toml` unless explicitly overridden.
6. The CLI launch path and mount helpers consume these synced
   configurations so runtime clients can discover all MCP endpoints even when they
   still read the legacy home-root path during migration.
7. Foreground Codex resume now keeps the stored thread UUID but will temporarily
   launch against legacy `~/.codex` when that rollout exists there and is absent
   from the shared `~/.rust-chatbot/.codex` home. This preserves imported or
   externally-created Codex threads without changing the stored resume handle.
8. For foreground chat workers, provider request overrides now stage the
   request `working_dir` itself into the sandbox workspace before any extra
   `runsc_dirs`, so the container always sees the active checkout instead of an
   empty staging root when runsc is enabled from the UI. That override path is
   keyed off the effective provider runsc config, not only explicit
   `request.json` runsc fields, so Gemini's default runsc image still triggers
   workspace staging. In the desktop UI, the first runsc directory selected for
   a chat also becomes that chat's workspace root, so the first sandbox mount
   and the staged `working_dir` stay aligned by default. Once the first user
   message is sent, the header hides the `Runsc Dirs` row entirely when no
   runsc directories were configured for that chat. The same header now renders
   the chat title, workspace row, and visible `Runsc Dirs` row without the old
   `egui::group(...)` outline while preserving the original `6px` inner padding,
   existing controls, and lock behavior. Within the unlocked `Runsc Dirs` UI,
   only the top-level `Add Folder` button is allowed to append a new row; the
   per-row recent-folder menu and `Open Folder...` action replace the selected
   row in place, and the empty-state recent menu only prefills the input box.
   The same session state now also persists an optional `runsc_image`
   override, exposes it beside the `Runsc Dirs` header row in the desktop UI,
   and forwards it through `WorkerRequest.runsc_image` so a chat can override
   the provider-default runsc image without changing the global worker config.
   The shared
   `ai-providers` staging layer now normalizes an
   absolute workspace-root include back to `"."` and omits repo-local runtime
   state roots such as `target/`, `.claude-desktop/`, `.gemini/`, `.nvidia/`,
   `.qwen/`, and related provider cache directories during full-workspace
   staging. That avoids both large build-artifact copies and unreadable Rust
   incremental lockfiles under `target/debug/incremental`.
9. After the subprocess exits, `RunscWorkspaceSync::sync_back()` copies changed
   workspace files from the sandbox back into the host workspace.

Persisted `ChatSession` records now treat `runsc_dirs` as part of the baseline
session schema even when no extra runsc mounts were configured. That keeps
session-inspection paths such as `session_mcp` fixtures and serialized test
stores aligned with the runtime session model instead of depending on a
pre-runsc session shape.

When a provider subprocess exits non-zero, the failure summary preserves the
exit status even when stderr is present so operator-visible errors distinguish
transport crashes from plain stderr logging. The terminal adapter error wrapper
also normalizes repeated leading `Error:` prefixes from both streamed output
and stderr fallback text before it emits the final `exit_code:<n>: ...`
message into the UI/session transcript.

Autonomous coordination runs layer one extra mode on top of this generic
background path. When `RUST_CHATBOT_AUTONOMOUS_RUNSC_SHARED_ROOT` is present,
the sandbox wrapper reuses that existing run-scoped workspace instead of
creating a fresh temp staging dir, and
`RUST_CHATBOT_AUTONOMOUS_RUNSC_DISABLE_SYNC_BACK=1` disables direct
container-to-host sync-back. In that mode, Rust captures changed files from the
shared sandbox workspace and materializes them back into the host checkout
after task completion.

## Runsc Isolation

`../chatbot-worker/src/provider_bridge/autonomous.rs` turns on `runsc` for
background work when either `RUST_CHATBOT_AUTONOMOUS_EXECUTION_ENV=runsc`
(set by the run orchestrator's `AutonomousRunscEnvGuard`) or
`RUST_CHATBOT_BACKGROUND_EXECUTION_ENV=runsc` is present. Both are checked;
the orchestrator variable takes precedence when only it is set. In that mode
it injects:

- `background_runsc_image` from config, defaulting to
  `rust-chatbot-autonomous-runsc:latest`
- `RUST_CHATBOT_AUTONOMOUS_RUNSC_WORKSPACE_INCLUDES` entries for extra
  workspace paths that must be mounted read-write

When the provider process is already running inside the outer autonomous
worker container, `ai_providers::sandbox::wrap_command_with_runsc(...)` marks
that environment with `RUST_CHATBOT_AUTONOMOUS_INSIDE_RUNSC=1`. The provider
override layer treats that marker as authoritative and suppresses any inner
provider-level `runsc` wrapping, so autonomous workers do not require nested
`docker`/`runsc` tooling inside the container.

That image must remain on a glibc `2.39+` userspace because autonomous
`runsc` launches bind-mount host-built `auto-runs` binaries into the
container. A bookworm-era glibc `2.36` image can accept the mount layout and
still fail immediately when the worker binary starts, producing `GLIBC_2.39`
loader errors and leaving the orchestrator PID to expire the claim lease.
`ai_providers::sandbox::wrap_command_with_runsc(...)` now enforces that
compatibility explicitly by probing both the host and image glibc versions
before it rewrites the command; an underspecified image fails fast with a
clear preflight error instead of consuming task attempts through lease expiry.

The wrapper also avoids bind-mounting host system binary roots such as
`/usr/bin` or `/usr/local/bin` into the container when a command resolves to a
standard image-provided tool like `python3`. Mounting those directories can
shadow the image's own entrypoint binaries such as `/usr/bin/tini`; only
non-standard install roots (for example `asdf` or repo-local toolchains) are
mounted through for command execution.

`autonomous_worker_runsc_mounts()` in `auto-runs` populates the extra mounts
passed to the outer `wrap_command_with_runsc` call that wraps the worker binary.
For provider CLI tools it now calls
`ai_providers::sandbox::program_directory_mounts(cli_name)`, which resolves the
binary via `PATH`, follows symlinks, and mounts the common ancestor of the
symlink directory and the real target directory — typically the entire asdf node
installation root (`~/.asdf/installs/nodejs/<version>`). This makes `node` and
all `bin/` scripts available inside the container so the worker can spawn
provider CLIs without a nested `runsc` call.

Adapter-backed worker launches also treat one transport-marked provider failure
as retryable when that failed attempt emitted no assistant text yet. The worker
records those retries in the per-chat spool at `transport-errors.log`, mirrors
them into the rotating app log, and appends a diagnostic live-summary row into
`events.jsonl` so a single transport glitch does not immediately hard-fail the
chat turn.

Qwen is now OpenRouter-backed in the runtime catalog. Background Qwen runs use
`openrouter-cli exec` with the `qwen/qwen3-coder:free` model and reuse the
`qwen_session_id` field for resume metadata so the user-facing provider remains
`qwen` even though the transport is OpenRouter.

Provider MCP config sync still strips the `bash` and `mcp-server-commands`
servers when sandboxed `runsc` execution is active, alongside the host-rooted
filesystem server rooted at `/`. Host-side provider configs now keep that
filesystem server pointed at `/` so agents can inspect runtime logs and state
outside `/home/example/git`, but provider command construction still appends a
build/test-specific instruction that routes compilation through the tracked
`lightweight-builds` MCP surface and test execution through the tracked
`lightweight-tests` surface instead of shell MCP tools.

The public `mcp.example.json` shows the portable memory-server surface.
Build/test MCP servers such as `lightweight-mcp` are optional external tools;
install them and configure their executable paths, allowed roots, and timeouts
for your environment. They are not bundled in this release set. Gemini's runsc
path omits MCP entries that depend on host-only executables or absolute paths.

## Transport-Family Builders

Provider-specific CLI config construction and runsc mount generation are
dispatched through four active **transport-family builders** in
`../chatbot-worker/src/provider_bridge/`. Each builder handles the shared
transport logic for its family, with targeted hooks for provider-specific
behavior. The `TransportFamily` for each provider is defined in the runtime
catalog (`../chatbot-worker/src/provider_catalog.rs`).

| Family | Providers | Mount behavior |
|--------|-----------|---------------|
| **ClaudeCompat** | Claude, MiniMax | Mount Claude-style config dirs and auth files. |
| **CodexCompat** | Codex, Nvidia | Mount a writable `.codex` sidecar directory seeded from the shared `~/.rust-chatbot/.codex/` config root, with legacy repo-local fallback during migration. Nvidia uses a dedicated `CODEX_HOME` under `~/.rust-chatbot/.nvidia/codex-home/` with a custom model catalog and `supports_websockets = false`. Command launches also prepend the resolved CLI directory to `PATH` so shebang-based wrappers keep working when the pinned binary lives outside the ambient shell path. |
| **GeminiCli** | Gemini | Mount a writable global `.gemini` home for Gemini's project registry, then overlay the selected `settings.json` plus OAuth/state sidecars. |
| **OpenRouterCompat** | Qwen, OpenRouter | Use OpenRouter API credentials and the OpenRouter CLI adapter; no provider-specific runsc sidecars are required. |

For non-runsc interactive Gemini launches, the Gemini bridge now injects
`GEMINI_API_KEY` into the command environment when available from either the
process environment or `~/.rust-chatbot/.gemini/api.key`, which avoids interactive
OAuth prompts in non-terminal sessions.

MiniMax's Claude-compatible transport now uses MiniMax's documented Anthropic
endpoint path, `https://api.minimax.io/anthropic`, in the shared provider
catalog. That keeps the foreground and background ClaudeCompat bridge aligned
with the upstream Claude Code integration contract instead of routing MiniMax
through the generic `/v1` base URL.

Background model overrides and runsc mount dispatch are derived from the
catalog rather than scattered per-provider match tables. All providers also
mount the shared task-report validator script when it exists.

Gemini's runsc path now prefers an existing `.gemini` directory that already
contains `projects.json`. When the shared `~/.rust-chatbot/.gemini/` root has
not been seeded with that registry yet, the bridge falls back to the legacy
`~/.gemini/` home so Gemini cleanup and checkpoint storage can resolve the
staged runsc workspace without crashing inside `ProjectRegistry`.

Runsc launches now always set `TINI_SUBREAPER=1` to suppress warning spam from
`tini` when container entrypoints nest process managers, and Gemini mounts are
seeded into a writable sidecar directory chosen from the shared infra path with
`/tmp` fallback to avoid stale root-owned sidecar state preventing writes to
`~/.gemini/tmp/workspace/chats`.

Gemini's runsc MCP sync is narrower than the generic runsc filter. After the
shared shell/filesystem removals, Gemini now drops the checked-in Rust MCP
replacements as well because the shared `.mcp.json` launches them through
repo-local `cargo run --manifest-path ...` commands under
`/workspace/mcp-servers/`. Those commands depend on host repo paths and a
Cargo toolchain the runsc image does not provide, so they are omitted from the
Gemini runsc sidecar instead of being copied into a sandbox where they cannot
start.

The desktop status bar now uses that same provider-aware effective MCP set when
it renders the bottom-panel `MCP: N` count. Gemini therefore shows the
runsc-portable subset instead of the larger shared `.mcp.json` inventory, while
other providers still display the generic filtered set.

Structured-output overrides flow through the same family builders. For
Claude-compatible transports, `output_json_schema` stays inline on the
`ClaudeCliConfig` and the builder injects `ANTHROPIC_REASONING_EFFORT` from the
per-provider mapping (`claude` => `high`, `minimax` => `xhigh`, by default).
For Codex-compatible transports, the bridge persists the schema under
`~/.rust-chatbot/provider-bridge-output-schemas/<hash>.json` and,
when `runsc` is active, mounts that file at the same path inside the sandbox so
the pinned Codex CLI can read a stable host-backed schema file. The background
executor's manual Claude/Codex command builders also forward those settings to
the subprocess as `--json-schema` / `--output-schema`; autonomous runs do not
reuse the shared `ai-providers` command-construction path.

The legacy `QwenCli` helper still exists for migration fallback from old
repo-local `.qwen` sidecars, but no checked-in provider currently routes
through that transport family.

For the `CodexCompat` family, production resolution now uses the ambient system
`codex` command. Background and autonomous workers no longer pin to the
repo-local custom build, so Codex-family launches follow the operator's
installed Codex while the worker continues to manage provider-specific config
such as `CODEX_HOME`.

When `runsc` is active, the CodexCompat mount builder no longer overlays
`config.toml` and `auth.json` into `CONTAINER_HOME/.codex` as separate file
bind mounts. It stages a writable sidecar directory under the shared
`~/.rust-chatbot` infrastructure root, seeds missing `config.toml` and
`auth.json` from the shared `~/.rust-chatbot/.codex/` source, falling back to
legacy repo-local config when needed during migration, and mounts that single
`.codex` directory into container home. That keeps Codex's temp-file plus
rename persistence path within one writable directory instead of crossing mount
boundaries at `.codex/config.toml`.

Host-side Codex runs now use that same shared provider root as their explicit
`CODEX_HOME` instead of inheriting the operator's ambient `~/.codex`. The
provider bridge also collapses every top-level Codex MCP definition into one
visible `mcp-coordinator` entry: provider-scoped MCP servers from `.mcp.json`
plus local `.codex/config.toml` entries such as `lightpanda` and `taskserver`
are written into the coordinator upstream sidecar, and the rewritten
`config.toml` keeps only the coordinator MCP block alongside the remaining
non-MCP Codex settings.

## Prompt And Workspace Behavior

`../chatbot-worker/src/worker_ext/background/executor.rs` rewrites absolute
workspace paths in prompts when `runsc` is enabled so the provider sees the
sandbox workspace path instead of the host path. That rewrite is
path-boundary-aware: it rewrites the workspace root itself and descendants,
but does not mangle sibling paths that merely share the same prefix. It also
normalizes a trailing path separator on the configured workspace root so
`/repo` and `/repo/` rewrite identically. The same executor also skips the
Codex git-repo check prefix handling that is only valid on the host path.

## Current Scope

This path executes background provider calls and can isolate them with `runsc`,
but the old persisted autonomous-run ledger and SQLite analytics tables are no
longer written by the current tree.

## Verification

The current `runsc` background path is covered by focused unit tests in two
areas:

- Provider override and mount coverage in
  `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs`
- Command-building and prompt-rewrite coverage in
  `../chatbot-worker/src/worker_ext/background/tests/command_building.rs`

Provider-dispatch tests can also force all background CLI launches through one
fake executable by setting `RUST_CHATBOT_TEST_PROVIDER_CLI`. That keeps the
bounded-concurrency and stderr/exit-code tests independent from any particular
provider-specific CLI path override.

Named tests that guard the current behavior:

- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::autonomous_overrides_enable_runsc_when_requested`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::autonomous_overrides_include_runsc_workspace_paths_when_requested`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::provider_config_claude_preserves_output_json_schema_override`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::claude_compat_config_sets_anthropic_reasoning_effort_env`
- `../chatbot-worker/src/provider_bridge/tests/mcp_sync.rs::sync_mcp_configs_targets_minimax_compat_json`
- `../chatbot-worker/src/provider_bridge/tests/mcp_sync.rs::sync_mcp_configs_targets_shared_and_legacy_codex_paths`
- `../chatbot-worker/src/provider_bridge/tests/mcp_sync.rs::sync_mcp_configs_respects_explicit_codex_toml_override`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::provider_config_codex_persists_output_schema_and_mounts_it`
- `../chatbot-worker/src/worker_ext/background/tests/command_building.rs::build_claude_command_includes_json_schema_flag`
- `../chatbot-worker/src/worker_ext/background/tests/command_building.rs::build_codex_command_includes_output_schema_flag`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::codex_runsc_mounts_include_repo_local_auth_sidecar`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::claude_runsc_mounts_include_container_home_auth_sidecars`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::minimax_runsc_mounts_include_container_home_sidecars`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::gemini_runsc_mounts_include_repo_local_auth_sidecars`
- `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::autonomous_overrides_sets_model_for_all_providers`
- `../chatbot-worker/src/worker_ext/background/tests/command_building.rs::build_provider_command_covers_qwen_and_gemini_paths`
- `../chatbot-worker/src/worker_ext/background/tests/command_building.rs::rewrite_runsc_workspace_paths_rewrites_absolute_workspace_mentions`
- `../chatbot-worker/src/worker_ext/background/tests/command_building.rs::build_codex_command_skips_git_repo_check_in_runsc_mode`

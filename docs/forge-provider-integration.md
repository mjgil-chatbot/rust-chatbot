# Forge Provider Integration

This document records the full path used to add Forge-backed
`GeminiForge` and `MiniMaxForge` support without changing the existing native
`gemini` and `minimax` routes.

The work spans four repositories:

| Repo | Responsibility |
| --- | --- |
| `../others/forgecode2` | Expose a `rust-chatbot`-compatible compatibility binary on top of Forge |
| `../chatbot-worker` | Treat the Forge providers as first-class worker providers and build the subprocess commands |
| `.` | Surface the providers in `rust-chatbot`, `rcc`, `chatbot-ctl`, session storage, and verification scripts |
| `../auto-ui` | Accept the new provider names in the rust-chatbot adapter and exercise the UI/debug flows |

## 1. Forge Compatibility Surface

The first step was to make Forge look like a `rust-chatbot` provider subprocess
instead of trying to change `rust-chatbot` to understand Forge's native CLI.

Implementation lives under `../others/forgecode2/crates/forge_main/src/rust_chatbot/`:

- `cli.rs`
  - add a separate `rust-chatbot` binary surface
  - support both the bare prompt form and `exec`
  - accept the worker-facing flags `--model`, `--resume`, `--workspace-root`,
    `--context-window-size`, `--mcp-config`, `--sandbox`, `--base-url`, and
    `--json`
  - normalize both invocation forms into one `ResolvedCliRequest`
- `config.rs`
  - load chatbot-format config from `--config`, `CHATBOT_CONFIG`, shared config
    files, and workspace-local `.chatbot/config.toml`
  - resolve `provider:model` targets plus bare-model overrides when the provider
    can be inferred
  - map `minimax_forge` to Anthropic-compatible transport and `gemini_forge` to
    Google-compatible transport
  - provide env-key fallback for the compatibility providers, including
    `MINIMAX_API_KEY`, `GEMINI_API_KEY`, `GOOGLE_API_KEY`, and
    `GOOGLE_GENERATIVE_AI_API_KEY`
- `session.rs`
  - persist local compatibility sessions under
    `~/.rust-chatbot/.forgecode2-rust-chatbot-compat/sessions/`
  - make `--resume` work even though the upstream Forge session ids are not the
    same objects as `rust-chatbot` provider resume handles
  - when a requested resume id is missing, create a new empty compatibility
    session with that same id instead of failing the worker flow
- `stream.rs`
  - stream Anthropic-compatible, OpenAI-compatible, and Google-compatible SSE
    responses
  - rebuild request history from the persisted compatibility session before each
    resumed call
  - emit `rust-chatbot`-style JSONL with `response`, `command`, `session_id`,
    `summary`, and `is_final`
  - persist the final user/assistant turn back into the compatibility session
- `mod.rs`
  - glue the pieces together: resolve prompt, load config, resolve provider
    target, load or create a compatibility session, and execute the request

This compatibility layer was intentionally isolated in `crates/forge_main`
instead of modifying the interactive Forge startup path.

## 2. Worker Integration

Once Forge could speak the expected subprocess contract, the next step was to
route separate worker providers to that binary instead of reusing the native
Gemini and MiniMax entries.

### Provider identity and catalog

In `../chatbot-worker/src/model.rs` and
`../chatbot-worker/src/provider_catalog.rs`:

- add `ModelProvider::GeminiForge`
- add `ModelProvider::MiniMaxForge`
- wire them into `ModelProvider::ALL`
- map them to the existing `gemini_session_id` and `minimax_session_id` fields
  on `ChatSession`
- assign distinct shared-memory ids and CLI aliases:
  - `geminiforge`, `gemini-forge`, `gemini_forge`
  - `minimaxforge`, `minimax-forge`, `minimax_forge`
- keep them separate from the native providers by giving them their own catalog
  entries, display names, app ids, config directories, and data directories

The catalog routes both providers through
`TransportFamily::OpenRouterCliCompat`. That is the important design choice:
the worker treats these providers as subprocess-compatible JSONL transports,
not as direct Gemini CLI or Claude CLI transports.

### Provider bridge

In `../chatbot-worker/src/provider_bridge/openrouter.rs`:

- resolve API keys for the Forge providers from their own provider slots rather
  than from the shared OpenRouter key path
- prefix model names as `gemini_forge:<model>` and `minimax_forge:<model>` when
  the requested model is not already fully qualified
- route the CLI path to the Forge compatibility binary instead of
  `openrouter-cli`
- pass through `resume_session_id`, `workspace_root`, `mcp_config`, and the
  provider-specific auth env needed by Forge

In `../chatbot-worker/src/worker_ext/background/executor.rs`:

- treat `GeminiForge` and `MiniMaxForge` as worker providers that launch through
  the OpenRouter CLI compatibility branch
- build `rust-chatbot exec` commands with:
  - `--model`
  - `--resume`
  - `--workspace-root`
  - `--context-window-size`
  - `--mcp-config`
  - `--sandbox workspace-write`
  - `--json`
- keep the existing native Gemini and MiniMax routes untouched
- fail early with provider-specific auth messages if the Forge API key is
  missing

This is the point where the Forge providers became real worker-backed providers
instead of direct CLI experiments.

## 3. rust-chatbot Integration

With the worker catalog and subprocess bridge in place, `rust-chatbot` still
needed to expose the providers through its own launch and inspection surfaces.

### Provider exposure

In `src/bin/rcc.rs`, `src/bin/chatbot_ctl/usage.rs`, and
`src/markdown.rs`:

- accept `geminiforge` and `minimaxforge` as launcher arguments
- include them in the usage text so `rcc GeminiForge` and
  `rcc MinimaxForge` resolve to real providers

In `src/bin/chatbot_ctl/broker.rs`:

- assign default instance ids
  - `GeminiForge` -> `9`
  - `MiniMaxForge` -> `10`
- allow `chatbot-ctl launch --provider geminiforge`
- allow `chatbot-ctl launch --provider minimaxforge`

The launch path depends on a shared provider gate in
`../chatbot-worker/src/model.rs`, not only on the local broker/parser code.
Forge launchers must stay in the launchable-provider set used by
`rust-chatbot`, `rcc`, and `chatbot-ctl`, while the narrower adapter-backed
provider set remains limited to the providers that already run through
`agent-adapter-cli`. If those two sets drift together again, Forge launchers
either regress back to `Unknown provider` or get routed through the wrong
runtime path.

In `src/bin/chatbot_ctl/desktop.rs` and `src/main.rs`:

- add desktop entries for the new providers
- map them to the existing Gemini and MiniMax icon assets
- include the provider-specific app ids and titles used for window matching

### Session/runtime surfaces

The runtime now treats the Forge providers as regular `rust-chatbot` providers
for session storage and UI state:

- `GeminiForge` sessions live under `~/.rust-chatbot/.gemini-forge-desktop/`
- `MiniMaxForge` sessions live under `~/.rust-chatbot/.minimax-forge-desktop/`

The UI state helpers and normalization surfaces already use the shared
`ModelProvider` accessors, so once the new providers were added to the catalog
and model layer they participated in the same session-id plumbing.

### Streamed plaintext rendering contract

The following describes the selected source snapshot.

Forge-backed providers can mix three different payload styles in one
turn:

- transcript/status preamble lines such as `● [03:11:39] Read README.md`
- plain text bodies containing ANSI spinner noise or unsupported private-use
  glyphs
- fenced code blocks embedded inside otherwise plaintext streamed output

`rust-chatbot` now treats that stream as a normalized plaintext transport:

- `src/forge_loop.rs`
  - strips ANSI control sequences and spinner/status frames
  - removes private-use glyphs that render as empty squares in the desktop font
  - maps box-drawing tables to ASCII so streamed Muse/Forge plans remain legible
- `src/app/ui/message.rs`
  - keeps transcript/status preambles in the plaintext renderer
  - splits real markdown-style response bodies away from transcript preambles
    when the body contains headings, lists, tables, or fenced code
  - prepares plaintext render blocks explicitly so fenced code is rendered as a
    distinct UI block instead of being left embedded in monospaced text
  - syntax-highlights fenced code blocks even when the surrounding stream stays
    in the plaintext renderer, and wraps them in a dedicated framed background

This contract is what keeps `:forge` and `:muse` streamed turns readable in the
desktop UI without forcing raw transcript lines through the markdown renderer.
If a provider emits fenced code blocks, `rust-chatbot` renders them through the
dedicated plaintext code-block path with syntax highlighting and a framed
background. If a provider emits only plain text such as `fn main() {}`, the UI
preserves that literal plaintext rather than inventing a fence.

### Debug and automation support

The repo-local scripts also needed to recognize the new providers:

- `scripts/auto_ui_debug.py`
- `scripts/auto_ui_header_debug.py`
- `scripts/auto_ui_debug_support.py`
- `scripts/restart-provider.sh`

The last missing piece was `scripts/auto_ui_prompt_debug.sh`. Its environment
recovery probe originally searched only for argv entries containing the direct
repo binary path. That fails once the live process is a wrapper named
`rust-chatbot-geminiforge` or `rust-chatbot-minimaxforge`.

The fix was to search for both:

- `<repo>/target/release/rust-chatbot` and `<repo>/target/debug/rust-chatbot`
- `rust-chatbot-${provider}`

That lets the script recover `DISPLAY`, `XAUTHORITY`, and `XDG_RUNTIME_DIR`
from a live provider process before launching `auto-ui`.

## 4. auto-ui Integration

`rust-chatbot` script support was not enough on its own because `../auto-ui`
also had to understand the new provider names and session layout.

In `../auto-ui/crates/auto-ui-adapter-rust-chatbot/`:

- add `Provider::GeminiForge`
- add `Provider::MiniMaxForge`
- accept the Forge provider aliases in clap and serde
- map them to:
  - `.gemini-forge-desktop` / `gemini_session_id`
  - `.minimax-forge-desktop` / `minimax_session_id`
- extend session-metadata parsing so mixed provider inventories include the
  Forge variants

That adapter file had become too large, so the provider work shipped with a
split into smaller `src/adapter/*.rs` and `src/tests/*.rs` modules to keep the
Rust files well below the 800-line limit.

## 5. Verification Path

The integration was validated from the bottom up.

### Forge compatibility checks

The Forge compatibility target carries direct unit coverage for:

- CLI normalization in `cli.rs`
- provider/model resolution in `config.rs`
- missing resume fallback and session persistence in `session.rs`
- JSONL stream output shape in `stream.rs`

These tests prove the compatibility binary can accept the worker contract before
it is launched by `rust-chatbot`.

### Worker and launcher checks

The worker and launcher changes were validated with targeted release tests:

- `../chatbot-worker/src/provider_bridge/tests/transport_family.rs::forge_providers_use_openrouter_cli_transport_with_prefixed_model_targets`
- `src/bin/rcc.rs::parse_args_accepts_all_provider_names`

These cover the two main integration risks:

- the worker must build the Forge subprocess commands through the new transport
  family
- the `rust-chatbot` launcher layer must accept the new provider names

### End-to-end interactive checks

Interactive verification requires separately installed providers and automation tools.
Exercise the live path with:

1. direct Forge compatibility CLI smoke tests against a local mock upstream
2. detached worker sends through `chatbot-ctl send`
3. repo-local `auto-ui` header and layout debug flows
4. repo-local `auto_ui_prompt_debug.sh` runs for both providers

The `auto-ui` adapter verification used these targeted tests:

- `../auto-ui/crates/auto-ui-adapter-rust-chatbot/src/tests/mod.rs::forge_providers_map_to_rust_chatbot_metadata`
- `../auto-ui/crates/auto-ui-adapter-rust-chatbot/src/tests/metadata.rs::read_sessions_metadata_all_provider_types`

### Stream-hardening regression checks

Native regressions cover the mixed
plaintext stream path:

- `src/forge_loop.rs::sanitizes_full_muse_transcript_with_table_and_fenced_code`
- `src/app/ui/message.rs::transcript_preamble_with_ascii_table_and_fenced_code_splits_rendering`
- `src/app/ui/message.rs::prepare_plaintext_render_blocks_extracts_fenced_code_for_ui_path`
- `src/app/ui/message.rs::prepare_plaintext_render_blocks_normalizes_plaintext_code_language`
- `src/app/ui/message.rs::prepare_plaintext_render_blocks_keeps_transcript_lines_outside_code_block`

These tests lock in the combined behavior that the UI depends on:

1. transcript/status lines remain plaintext
2. ANSI/private-use/box-drawing artifacts are normalized before persistence and
   rendering
3. fenced code blocks survive the sanitized plaintext path intact

The repo-local `auto-ui` assertion helper also carries a prompt-debug mode for
this path:

- `scripts/auto-ui-assertions.py --prompt-debug-session-json <session.json> ...`
- `scripts/auto-ui-test-prompt-debug.sh --session-id <id> --session-json <path> ...`

That mode validates the production artifact shape directly from
`report.json` plus the saved session:

- the `prompt_debug` run completed successfully
- the UI stayed on the plaintext-stream renderer (`last_markdown_row == null`)
- the screenshot artifact exists
- the final assistant message contains no private-use or box-drawing glyphs and
  still includes required ASCII/code markers

The wrapper script is the production entry point for this check. It can launch
the live prompt-debug flow and immediately assert the output, or reuse an
existing artifact directory via `--assert-only` for deterministic replay.

For release verification, the broader `scripts/auto-ui-test-full.sh` suite now
has an opt-in hook for this real-provider check. When
`PROMPT_DEBUG_SESSION_ID` plus `PROMPT_DEBUG_SESSION_JSON` are set, the suite
runs its fixture-backed scenarios first and then invokes the prompt-debug
wrapper so the Forge/Muse plaintext contract participates in the same top-level
verification command.

Example verification command (supply your own local session and output directory):

```bash
scripts/auto-ui-test-prompt-debug.sh \
  --session-id <session-id> \
  --session-json <session-json-path> \
  --provider minimaxforge \
  --output-dir <output-dir> \
  --require-substring 'fn main()'
```

Inspect `report.json` in the selected output directory: require `status: "ok"`,
`last_markdown_row: null`, a normalized saved session, and a readable screenshot.
Run these checks in your own environment; private live-run artifacts are not
part of this source distribution.

## 6. Final State

After this integration:

- Forge exposes a dedicated `rust-chatbot` compatibility binary
- `chatbot-worker` treats `GeminiForge` and `MiniMaxForge` as real providers
  with their own catalog entries
- `rust-chatbot` can launch, inspect, and store sessions for the Forge-backed
  providers without changing the native Gemini or MiniMax routes
- `auto-ui` and the repo-local debug scripts can verify both providers through
  the same flows used for the rest of the provider matrix

# Autonomous Coordination Architecture

This document describes the provider infrastructure that supports autonomous
multi-agent coordination. For the full canonical design rationale, see
[`../autonomous/design-doc.md`](../autonomous/design-doc.md).

## Runtime Catalog vs. Autonomous Policy Subset

The provider system maintains two distinct data sources:

### Runtime Catalog (`../chatbot-worker/src/provider_catalog.rs`)

The **runtime catalog** is a compile-time `const` array of `ProviderEntry`
values covering all seven providers: Claude, MiniMax, Codex, Gemini,
Qwen, Nvidia, and OpenRouter. It is the single source of truth for provider
metadata — names, display names, SHM codes, transport families, CLI binaries,
API key env vars, config paths, and normalization rules.

`ModelProvider` methods (`as_str()`, `display_name()`, `from_alias()`,
`shm_code()`, etc.) delegate to catalog lookups rather than maintaining
independent match arms. Config-dir resolution, app titles, and data-dir names
also resolve from the catalog.

### Autonomous Policy Subset (`workflow-registry/providers.json`)

The **autonomous policy subset** is a JSON file listing only the providers
approved for autonomous multi-agent execution. It is embedded at compile time
via `../chatbot-worker/src/config/mod.rs` and used by the coordination engine to decide which
providers may participate in background runs.

When a profile sets `model_override`, that value is a raw provider model
identifier. It is kept in `workflow-registry/providers.json` rather than the
runtime catalog, so policy overrides must be updated explicitly and should not
contain terminal formatting sequences or presentation-only suffixes.

The current approved registry includes: **Claude, MiniMax, Codex, Gemini,
Nvidia, OpenRouter, and Qwen**.

This split means adding a provider to the catalog does not automatically define
its autonomous policy — that still requires an explicit entry in
`workflow-registry/providers.json`.

## Transport Families

Provider CLI config construction is organized into five **transport families**,
implemented across the provider bridge module
(`../chatbot-worker/src/provider_bridge/`).
The `TransportFamily` enum in the catalog determines which builder handles a
given provider:

| Family | Variant | Providers | Transport |
|--------|---------|-----------|-----------|
| **ClaudeCompat** | `TransportFamily::ClaudeCompat` | Claude, MiniMax | Claude CLI against native or Anthropic-compatible endpoints |
| **CodexCompat** | `TransportFamily::CodexCompat` | Codex, Nvidia | Codex CLI with optional custom `CODEX_HOME` |
| **GeminiCli** | `TransportFamily::GeminiCli` | Gemini | Gemini CLI with OAuth and settings sidecars |
| **OpenRouterCompat** | `TransportFamily::OpenRouterCompat` | Qwen, OpenRouter | OpenRouter API / CLI adapter configuration |
| **QwenCli** | `TransportFamily::QwenCli` | None in the checked-in catalog | Legacy native Qwen CLI support |

Each family builder handles:

- **Interactive config**: Building the `ProviderConfig` for the ai-providers
  crate.
- **Background overrides**: Applying autonomous model overrides and execution
  flags.
- **Runsc mounts**: Constructing the bind-mount list for sandboxed execution,
  including provider-specific auth and settings sidecars.

Provider-specific hooks live inside their family builder rather than in
separate per-provider functions:

- **Nvidia** (CodexCompat): Bootstraps a dedicated `CODEX_HOME` under `.nvidia/codex-home/` with a
  custom model catalog to avoid NVIDIA's incompatible `/models` endpoint.
- **Gemini** (GeminiCli): Mounts `settings.json` and OAuth state files.
- **Qwen** (OpenRouterCompat): Uses OpenRouter transport with the
  `qwen/qwen3-coder:free` default model while preserving `qwen` as the
  user-facing provider ID and session field namespace.
- **OpenRouter** (OpenRouterCompat): Uses the OpenRouter CLI and direct API
  configuration for the generic OpenRouter provider.
- **QwenCli**: The legacy native-Qwen helper remains in the bridge for older
  sidecar/mount logic, but no checked-in provider currently routes through it.

## Session ID Management

Session IDs are managed through centralized accessors on `ModelProvider`:

- `ModelProvider::session_id(&self, session: &ChatSession) -> Option<&str>`
- `ModelProvider::set_session_id(&self, session: &mut ChatSession, id: String)`

These replace the scattered per-provider match arms that previously appeared in
`src/app/state/helpers_impl.rs`, `../chatbot-worker/src/memory/mod.rs`, and
`../chatbot-worker/src/worker_ext/background/`. The centralized accessors ensure all seven
providers are recognized consistently.

## Related Docs

- [Background Provider Runs](background-provider-runs.md) — execution flow,
  runsc isolation, and test coverage for background provider work.
- [Canonical Design Doc](../autonomous/design-doc.md) —
  full rationale and implementation plan for the catalog consolidation.
- [Adding a Provider](../adding-a-provider.md) — the five-step workflow for
  onboarding a new provider.

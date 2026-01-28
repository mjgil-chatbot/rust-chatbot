# Adding a New Provider

This document describes the registry-driven workflow for adding a new provider
to rust-chatbot. The consolidation behind this workflow is described in the
[canonical design doc](architecture/autonomous-coordination.md).

This is the only current provider-onboarding guide. The older scattershot
checklist was removed during documentation consolidation because it described
pre-catalog code paths that are no longer authoritative.

---

## Overview

Before the catalog consolidation, adding a provider required editing 20+ files
scattered across the codebase — roughly half compiler-enforced and half
runtime-only string matches that were easy to miss. The new design reduces this
to **five steps**, with most runtime metadata derived from a single catalog
entry.

---

## Step 1 — Add the `ModelProvider` enum variant

File: `../chatbot-worker/src/model.rs`

Add a variant to `enum ModelProvider` and include it in the `ALL` array. The
compiler will force you to handle the new variant in every exhaustive `match`
across the codebase (UI, worker, coordination, normalization). This is the
only step that creates compile errors — everything else flows from it.

```rust
pub enum ModelProvider {
    Claude,
    // ...
    NewProvider,  // ← add here
}
```

---

## Step 2 — Add a catalog entry

File: `../chatbot-worker/src/provider_catalog.rs`

Add a `ProviderEntry` to the `PROVIDER_CATALOG` array. This single entry
supplies all the runtime metadata that was previously spread across
`../chatbot-worker/src/config/constants.rs`,
`../chatbot-worker/src/config/paths.rs`, and
`../chatbot-worker/src/model.rs`:

| Field | Purpose |
|-------|---------|
| `id` | The `ModelProvider` variant |
| `name` | Lowercase canonical name (used in CLI parsing, serialization) |
| `display_name` | Human-readable label for the UI |
| `shm_code` | Unique `u8` for shared-memory provider identification |
| `transport_family` | One of `ClaudeCompat`, `CodexCompat`, `GeminiCli`, `OpenRouterCompat`, `QwenCli` |
| `interactive_model` | Model string for interactive (foreground) sessions — empty string means defer to CLI default |
| `background_model` | Model string used in background/autonomous runs — empty string means defer to CLI default |
| `cli_binary` | CLI executable name |
| `api_key_env` | Environment variable for the API key |
| `base_url` | API base URL (if applicable) |
| `config_dir_name` | Directory name under the user's home for provider config |
| `data_dir_name` | Directory name for session/data storage |
| `app_title` | Window / application title |
| `app_id` | Desktop application identifier |
| `aliases` | Alternative names accepted by the CLI parser |
| `normalization` | `NormalizationKind` controlling response normalization |

The catalog-backed helpers (`ModelProvider::as_str()`, `display_name()`,
`from_alias()`, `shm_code()`, config-dir resolution, etc.) pick up the new
provider automatically — no additional match arms needed in those functions.

---

## Step 3 — Wire session ID accessors

File: `../chatbot-worker/src/model.rs` (centralized accessors on `ModelProvider`)

Add arms to the centralized `session_id_from_chat()` and
`set_session_id_on_chat()` methods on `ModelProvider` so the runtime can read
and write the provider's session ID on `ChatSession`. These accessors are used
by the worker, the UI state helpers, memory detection, and the background
dispatcher — wiring them here replaces the half-dozen open-coded match arms
that the legacy checklist required.

Also add the corresponding `<provider>_session_id: Option<String>` field to
`ChatSession` in `../chatbot-worker/src/data.rs` and to `ProviderSession` in
the ai-providers crate. These remain `#[serde(default)]` optional fields.

---

## Step 4 — Add transport-family builder hook (if needed)

File: `../chatbot-worker/src/provider_bridge/config_builders.rs`

The provider bridge dispatches CLI config construction through four active
**transport-family builders** plus one retained legacy helper:

| Family | Providers | CLI transport |
|--------|-----------|---------------|
| **ClaudeCompat** | Claude, MiniMax | Claude CLI against native or compatible endpoints |
| **CodexCompat** | Codex, Nvidia | Codex CLI with optional custom `CODEX_HOME` |
| **GeminiCli** | Gemini | Gemini CLI with OAuth/settings sidecars |
| **OpenRouterCompat** | Qwen, OpenRouter | OpenRouter API / CLI adapter configuration |
| **QwenCli** | None in the checked-in catalog | Legacy native-Qwen helper retained for older sidecar logic |

If your new provider fits an existing family, the family builder handles it
with no code changes — the catalog's `transport_family` field is sufficient.

If the provider needs **provider-specific hooks** inside its family (custom
auth injection, or a custom `CODEX_HOME` like Nvidia), add a targeted hook
inside the relevant family builder. This keeps provider-specific logic
colocated with its transport family rather than scattered across the codebase.

If the provider uses an entirely new CLI transport, add a fifth family builder
and a new `TransportFamily` variant in the catalog.

---

## Step 5 — Enable for autonomous coordination (optional)

File: `../chatbot-worker/workflow-registry/providers.json`

If the new provider should participate in autonomous multi-agent runs, add it
to the autonomous policy subset in `workflow-registry/providers.json`. This
JSON file is the single source of truth for which providers are eligible for
autonomous routing. It is embedded at compile time via
`../chatbot-worker/src/config/mod.rs`.

Only providers that have been validated for autonomous execution should be
added here. The current approved subset is: Claude, MiniMax, Codex, Gemini,
Nvidia, OpenRouter, and Qwen.

---

## Verification

```bash
# 1. Compile — exhaustive matches catch missing arms
cargo build --release 2>&1 | head -50

# 2. Catalog coverage — every ModelProvider variant has a catalog entry
cargo test --release catalog_covers_all_providers

# 3. SHM code round-trip — no collisions or gaps
cargo test --release shm_code_round_trip

# 4. CLI alias parsing — the new provider is reachable
cargo test --release provider_alias_parsing

# 5. Config-path resolution — catalog-backed paths resolve correctly
cargo test --release config_path_resolution
```

---

## What changed

| Before (legacy) | After (catalog-driven) |
|-----------------|----------------------|
| ~20 files, ~50 touch-points | 5 steps, 3-5 files |
| Runtime-only string tables in constants, paths, CLI parsers | Single catalog entry drives all metadata |
| Per-provider config builder functions | Four transport-family builders |
| Scattered session-ID match arms | Centralized `ModelProvider` accessors |
| `config/provider-profiles.json` + `workflow-registry/providers.json` | Single `workflow-registry/providers.json` for autonomous policy |
| ~50% of sites were runtime-only and easy to miss | Catalog lookup eliminates most runtime-only gaps |

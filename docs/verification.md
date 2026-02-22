# Verification Contract

`rust-chatbot` no longer owns the entire runtime and autonomous stack in-tree.
Shared worker/runtime code now lives in the sibling `../chatbot-worker` repo,
autonomous workflow orchestration plus code-review planning now live in the
sibling `../auto-runs` repo, and adjacent UI, adapter, and history surfaces
now compile from sibling checkouts such as `../chatbot-ui`,
`../chatbot-adapter`, `../chatbot-history`, `../ai-providers`,
`../the-markdown-commons`, and `../agent-memory`.

This document keeps the verification surfaces that `rust-chatbot` still owns
directly and records the cross-repo contract points that must stay green across
the split. When a boundary changes, update this file and the owning sibling
repo's verification artifact in the same change.

## Cross-Repo Check

Use the shared verification helper in `../chatbot-worker` when validating repo
boundary changes:

```bash
../chatbot-worker/scripts/verify-sibling-builds.sh
```

Use `--build` for full builds or `--release` to add release-profile validation.

Forge-backed provider integration for `geminiforge` and `minimaxforge` is
documented separately in `./forge-provider-integration.md`, including the Forge
compatibility layer, worker transport wiring, launcher exposure, adapter tests,
and `report.json` artifact checks.

Prompt-debug UI verification for streamed Muse/Forge plaintext output is
asserted by `scripts/auto-ui-assertions.py` when called with
`--prompt-debug-session-json`. That mode validates:

- `scenario == "prompt_debug"` and `status == "ok"`
- `ai_response_end.status == "done"`
- `last_markdown_row == null` for plaintext-stream rendering
- the captured screenshot artifact exists and is non-trivial
- the latest assistant message in the saved session contains no private-use or
  box-drawing glyphs and still includes required ASCII table/code markers
- optional transcript-tool requirements can assert a minimum number of
  transcript-style tool lines and require specific command substrings inside
  those transcript lines
- optional transcript-tool requirements can also forbid raw tool substrings
  when provider adapters should surface normalized entries instead of generic
  labels such as `| Shell` or generic `| tool`
- transcript-tool detection accepts both bullet-prefixed prompt-debug streams
  and the standard timestamped tool rows persisted by current desktop
  provider runs such as MiniMax, while ignoring timestamped narrative lines
- when a streamed Forge/plaintext response later regresses to a body-only
  terminal payload, the saved session still retains the last streamed
  transcript preamble above that final body

The selected source snapshot includes the Forge/Muse streamed plaintext contract.

The preferred repo-local entry point for that verification is now:

```bash
scripts/auto-ui-test-prompt-debug.sh \
  --session-id <session-id> \
  --session-json <session-json-path> \
  --require-substring 'fn main()'
```

That wrapper launches `prompt_debug`, stores the artifact directory, and then
applies the prompt-debug assertions automatically. It also supports
`--assert-only` for replaying existing artifacts in unit tests or CI.

The engineering gate now uses a checked fixture pair for that replay mode:

```bash
scripts/auto-ui-test-prompt-debug.sh \
  --assert-only \
  --output-dir tests/fixtures/prompt_debug_assert \
  --session-json tests/fixtures/prompt_debug_assert/session.json \
  --require-substring 'fn main() {}' \
  --require-tool-transcript-count 2 \
  --require-tool-transcript-substring 'pwd' \
  --forbid-tool-transcript-substring '| Shell'
```

For end-to-end shell-tool rendering checks, the same wrapper can require
transcript lines explicitly:

```bash
scripts/auto-ui-test-prompt-debug.sh \
  --session-id <session-id> \
  --session-json <session-json-path> \
  --prompt 'Call the `mcp__bash__run` tool exactly 10 times ...' \
  --trace-timeout 180 \
  --require-tool-transcript-count 10
```

The same wrapper also supports negative transcript assertions for provider
tool-label regressions. MiniMax code-search verification should now reject the
generic `Shell` and `tool` labels directly while requiring an explicit
`Semble` tool row:

```bash
scripts/auto-ui-test-prompt-debug.sh \
  --session-id <session-id> \
  --session-json <session-json-path> \
  --provider minimaxforge \
  --prompt 'Use semble to find CLI argument parsing code and inspect the best match.' \
  --require-tool-transcript-count 3 \
  --require-tool-transcript-substring 'Semble' \
  --forbid-tool-transcript-substring '| tool' \
  --forbid-tool-transcript-substring '| Shell'
```

Example verification command (requires configured Forge tooling and your own session):

```bash
scripts/auto-ui-test-prompt-debug.sh \
  --session-id <session-id> \
  --session-json <session-json-path> \
  --provider minimaxforge \
  --output-dir <output-dir> \
  --require-substring 'fn main()'
```

Inspect the generated artifacts before treating a live run as passed:

- `report.json` status is `ok`
- `last_markdown_row` is `null`
- the saved session contains normalized plaintext without private-use or
  box-drawing glyphs
- the screenshot shows a readable plaintext stream layout

Private live-run artifacts are not included in this distribution.

Collapsed transcript-summary sanitizing and persisted tool-output regressions
are now covered by release-mode Rust tests in:

- `src/app/ui/message_tests.rs`
- `src/app/ui/transcript_summary.rs`
- `src/app/ui/message_list/tests/render_regressions.rs`

The checked fixture `tests/fixtures/real_extracted_tool_transcript_sample.md`
captures a real persisted transcript/body split so the desktop renderer keeps
escaped transcript rows from breaking the final markdown body layout.

For broader release verification, `scripts/auto-ui-test-full.sh` now supports an
opt-in real-provider tail:

- set `PROMPT_DEBUG_SESSION_ID`
- set `PROMPT_DEBUG_SESSION_JSON`
- optionally set `PROMPT_DEBUG_PROVIDER`
- optionally set `PROMPT_DEBUG_REQUIRE_SUBSTRING`
- optionally set `PROMPT_DEBUG_TRACE_TIMEOUT`
- optionally set `PROMPT_DEBUG_REQUIRE_TOOL_TRANSCRIPT_COUNT`
- optionally set `PROMPT_DEBUG_FORBID_TOOL_TRANSCRIPT_SUBSTRING`

When those env vars are present, the full auto-ui suite runs its normal fixture
coverage first and then invokes `scripts/auto-ui-test-prompt-debug.sh` as part
of the same verification command.

## UI-Lean Research Translation Contract

`~/git/ui-lean` is not part of the production build or runtime dependency
graph for `rust-chatbot`. When it is used, it must remain upstream research and
specification support unless its outputs are translated into repo-local checked
artifacts.

The current repo-local `ui-lean` documents are:

- `docs/architecture/ui-lean-integration-map.md`
- `docs/architecture/ui-lean-send-draft-stream-spec.md`

The acceptance rule is:

```text
ui-lean model
-> checked property or counterexample
-> distilled invariant
-> native rust-chatbot test, assertion, or runtime monitor
```

An experiment is not part of the app's accepted verification surface until it
ends in at least one native `rust-chatbot` artifact such as:

- a Rust unit or integration test
- a Python assertion test
- an auto-ui assertion
- a runtime monitor or timing assertion
- a pre-commit or engineering-gate verifier that rejects inline blocking UI-thread work
- a verification-doc entry mapping the invariant to named tests

The current native enforcement artifact for the background-offload discipline is
`scripts/verify_ui_thread_contract.py`, which does two things:

- resolves the `ui-lean` checkout from `--ui-lean-root`,
  `$RUST_CHATBOT_UI_LEAN_ROOT`, nearby repo candidates, or `~/git/ui-lean`, and
  then checks that its proof artifacts still report
  `no_blocking_work_on_ui_thread: proven`
- scans `rust-chatbot`'s protected frame-path files and rejects inline blocking
  filesystem/process/storage calls unless they are explicitly marked as
  background-only helpers

That verifier is enforced both by `.githooks/pre-commit` and by the
`ui_thread_contract` entry in `contracts/engineering-gate.toml`. The current
runtime follow-through for that contract is the async layout-log write path in
`src/app/ui/layout_log.rs` plus the background session height-cache load path
used during chat switches, which keep disk reads off the immediate
session-selection path. The uncached session-switch path also no longer waits
for a sidebar relist before applying the loaded transcript, and the main-panel
status bar now consumes a cached background probe for explicit `fff`
configuration instead of reading `mcp.json` during render, so the proof
translation now covers both background I/O and critical-path result shaping.

The fast auto-ui debug assertions in `scripts/auto-ui-assertions.py` now also
require both of the following behavioral orderings:

- at least one switched session must emit a `ui_auto_debug` trace while the
  height-cache background load is still pending and then later emit a second
  trace after that pending state clears
- the dedicated new-chat probe must render its first post-create
  `ui_auto_debug` trace with the created session already selected and already
  occupying sidebar index `0`

That gives the repo behavioral regressions for both "render first, hydrate
cache later" and "new chat appears in the sidebar immediately" alongside the
static ui-thread contract verifier.

The current native enforcement artifacts for the foreground send/draft/stream
spec are `scripts/verify_ui_spec_contract.py` and
`scripts/verify-ui-spec-regressions.sh`.

They enforce the repo-local translation of the bounded lifecycle spec by:

- checking that the spec docs still name the required lifecycle properties and
  their focused native enforcement artifacts
- checking that the required regression tests still exist at the expected
  source anchors
- running the exact release-mode regression bundle that covers `submit_valid`,
  stream takeover, empty-stream preservation, local and remote terminal
  cleanup, passive reload guards, and pure render non-mutation

That focused shell bundle also normalizes inherited toolchain-wrapper state
before running release regressions so the pre-commit and engineering-gate paths
do not depend on a caller-provided `RUSTC_WRAPPER`.

That verifier is enforced by `.githooks/pre-commit` and by the
`ui_send_draft_stream_contract` entry in `contracts/engineering-gate.toml`.

The same engineering gate now also has a diff-range mode for remote
verification. `.github/workflows/engineering-gate.yml` first clones the
current sibling path-dependency repos needed by this checkout's release-mode
regressions, then calls `scripts/verify-engineering-gate.sh --diff-base <sha>
--diff-head HEAD`. The gate's Python coverage now explicitly checks both named
command-group dispatch and diff-range file discovery in
`tests/test_engineering_gate.py`.

The first bounded interaction targeted for this workflow is the foreground
send/draft/stream/terminal lifecycle described in:

- `docs/architecture/ui-lean-send-draft-stream-spec.md`

That interaction currently maps to native regressions including:

- `src/app/state/session_runtime_tests.rs::apply_stream_to_current_clears_pending_draft_on_first_chunk`
- `src/app/state/session_runtime_tests.rs::local_terminal_worker_clears_pending_draft`
- `src/app/state/session_runtime_tests.rs::remote_terminal_update_clears_pending_draft`
- `src/app/state/ui_io_tests.rs::send_failed_clears_pending_draft_and_appends_error`
- `src/app/ui/message_list/tests/render_regressions.rs::render_with_pending_draft_does_not_clear_or_panic`

## Session/Worker Lifecycle Contract

The current native enforcement artifacts for the session/worker lifecycle
contract are `scripts/verify_session_lifecycle_contract.py` and
`scripts/verify-session-lifecycle-regressions.sh`.

They enforce the repo-local session/worker lifecycle contract by:

- checking that `docs/architecture/session-worker-lifecycle-spec.md` and this
  verification document still name the required lifecycle states, enforcement
  artifacts, and focused native anchors
- checking that the required lifecycle regression tests still exist at the
  expected source anchors
- running the exact release-mode regression bundle that covers pending-draft
  lifecycle entry, stream takeover, terminal cleanup, startup-gap pruning, and
  guarded current-session replacement behavior

That verifier is enforced by `.githooks/pre-commit` and by the
`session_worker_lifecycle` entry in `contracts/engineering-gate.toml`.

## Spool/Session Contract

The current native enforcement artifacts for the spool/session contract are
`scripts/verify_spool_session_contract.py` and
`scripts/verify-spool-session-regressions.sh`.

They enforce the repo-local spool/session contract by:

- checking that `docs/architecture/worker-spool.md`,
  `docs/architecture/spool-session-contract-versioning.md`, and this
  verification document still name the required versioning rules, enforcement
  artifacts, and focused native anchors
- checking that the required spool/session regression tests still exist at the
  expected source anchors
- running the exact release-mode regression bundle that covers unsupported
  schema-version rejection, terminal save ordering, think-only terminal
  preservation, request-only startup-gap pruning, stale provider-session
  replacement, and session MCP spool inspection metadata

That verifier is enforced by `.githooks/pre-commit` and by the
`spool_session_contract` entry in `contracts/engineering-gate.toml`.

## History/Checkpoint Contract

The current native enforcement artifacts for the history/checkpoint contract
are `scripts/verify_history_contract.py` and
`scripts/verify-history-regressions.sh`.

They enforce the repo-local history/checkpoint contract by:

- checking that `docs/architecture/chatbot-history-integration.md` and this
  verification document still name the required enforcement artifacts and
  focused native anchors
- checking that the required history/checkpoint regression tests still exist at
  the expected source anchors
- running the exact release-mode regression bundle that covers background
  history-refresh recovery, byte-for-byte store restore, checkpoint-manifest
  isolation, oversized/quota-bounded repo-patch omission, missing
  auto-checkpoint backfill, and checkpoint repo-summary projection

That verifier is enforced by `.githooks/pre-commit` and by the
`history_checkpoint_contract` entry in `contracts/engineering-gate.toml`.

## Memory Contract

The current native enforcement artifacts for the memory contract are
`scripts/verify_memory_contract.py` and
`scripts/verify-memory-regressions.sh`.

They enforce the repo-local memory contract by:

- checking that `docs/architecture/memmachine-integration.md` and this
  verification document still name the required enforcement artifacts and
  focused native anchors
- checking that the required memory regression tests still exist at the
  expected source anchors
- running the exact release-mode regression bundle that covers workspace memory
  status/search routing, persisted injection previews, retrieval audit
  candidate-plus-injected preview inspection, invalidation and restore flows,
  explicit-memory validation, and workspace-boundary enforcement

That verifier is enforced by `.githooks/pre-commit` and by the
`memory_contract` entry in `contracts/engineering-gate.toml`.

## History Auditor Contract

The current `rust-chatbot` enforcement artifacts for the history-auditor
integration are `scripts/verify_history_auditor_contract.py` and
`scripts/verify-history-auditor-regressions.sh`.

They now act as compatibility wrappers over the extracted sibling
`history-auditor` repository, which owns the actual implementation, native
regression tests, and contract checks.

- checking that `docs/architecture/history-auditor-v1.md` and this
  verification document still describe the extracted ownership model and the
  wrapper enforcement artifacts
- delegating to the extracted repo’s contract checker, which validates the
  standalone architecture docs, verification docs, and regression source
  anchors
- delegating to the extracted repo’s focused release-mode regression bundle
  that covers session audit persistence, public artifact-audit surface
  coverage, retrieval shaping, structured extraction, and repeated-incident
  memory behavior

## rust-chatbot Local Coverage

### Provider Boundary Parity

Parity coverage for the UI-facing provider boundary that still belongs to
`rust-chatbot`.

| Behavior | Named tests |
| --- | --- |
| Streaming text contract | `tests/parity_harness.rs::streaming_text` |
| Provider resume and session propagation | `tests/parity_harness.rs::resume_session_id` |
| Durable worker spool persistence during streaming | `tests/parity_harness.rs::worker_spool_persistence` |
| Provider malformed summary tolerance | `tests/parity_harness.rs::malformed_json_summary` |
| MCP tool round-trip handling at provider boundary | `tests/parity_harness.rs::mcp_tool_response` |
| Provider config injection and MCP config sync seams | `tests/parity_harness.rs::provider_config_injection` |
| Provider bridge request building normalizes successful streaming output | `tests/provider_bridge_stream.rs::provider_bridge_stream_builds_request_and_normalizes_success` |
| Provider bridge request building normalizes provider error output | `tests/provider_bridge_stream.rs::provider_bridge_stream_normalizes_provider_error_result` |
| Unsupported worker result schema versions surface as malformed terminal artifacts in the UI boundary | `src/app/state/helpers_impl.rs::read_worker_result_treats_unsupported_schema_version_as_malformed` |

### Session Selection And Draft Coverage

Sidebar highlight precedence, per-chat height-cache reload, and optimistic
draft visibility across chat switches.

| Behavior | Named tests |
| --- | --- |
| Sidebar highlight prefers the in-flight loading session over stale selected state | `src/app/ui/side_panel.rs::highlight_prefers_loading_over_selected`, `src/app/ui/side_panel.rs::highlight_prefers_loading_even_when_selected_matches_current` |
| Switching chats reload the selected session's persisted height cache without blocking activation on a sidebar relist | `src/app/state/session_runtime_tests.rs::switching_chats_loads_each_session_height_cache_in_background`, `src/app/state/ui_io_async_tests.rs::spawn_load_session_async_skips_sidebar_snapshot_reload_on_critical_path`, `src/app/state/ui_io_tests.rs::selected_session_reload_without_sidebar_patch_preserves_existing_sidebar_state` |
| Switching away from a chat with a draft hides that draft from the newly selected chat | `src/app/state/session_runtime_tests.rs::switching_chats_preserves_pending_draft_for_original_chat`, `src/app/ui/message_list/tests.rs::render_with_pending_draft_for_other_chat_hides_indicator` |
| Matching pending draft renders without being cleared during paint | `src/app/ui/message_list/tests.rs::render_with_pending_draft_does_not_clear_or_panic` |

### Prompt-Debug Assertion Coverage

Prompt-debug artifact assertions for Muse/Forge streamed plaintext UI runs.

| Behavior | Named tests |
| --- | --- |
| Prompt-debug plaintext stream accepts normalized ASCII output | `tests/test_auto_ui_assertions.py::PromptDebugAssertionsTests::test_prompt_debug_plaintext_stream_accepts_normalized_ascii_content` |
| Prompt-debug plaintext stream rejects residual box-drawing glyphs | `tests/test_auto_ui_assertions.py::PromptDebugAssertionsTests::test_prompt_debug_plaintext_stream_rejects_box_drawing_glyphs` |
| Prompt-debug plaintext stream can require transcript-style tool lines from both bullet and timestamped transcript formats while ignoring timestamped narrative lines | `tests/test_auto_ui_assertions.py::PromptDebugAssertionsTests::test_prompt_debug_plaintext_stream_accepts_required_tool_transcript_lines`, `tests/test_auto_ui_assertions.py::PromptDebugAssertionsTests::test_prompt_debug_plaintext_stream_accepts_timestamped_transcript_lines`, `tests/test_auto_ui_assertions.py::PromptDebugAssertionsTests::test_prompt_debug_plaintext_stream_rejects_missing_tool_transcript_lines` |
| Prompt-debug plaintext stream rejects regressed transcript tool labels such as raw MiniMax `Shell` lines and generic `tool` lines | `tests/test_auto_ui_assertions.py::PromptDebugAssertionsTests::test_prompt_debug_plaintext_stream_rejects_forbidden_tool_transcript_substring`, `tests/test_auto_ui_assertions.py::PromptDebugAssertionsTests::test_prompt_debug_plaintext_stream_rejects_generic_tool_label`, `tests/test_auto_ui_assertions.py::PromptDebugWrapperTests::test_assert_only_wrapper_rejects_forbidden_tool_transcript_substring`, `tests/test_auto_ui_assertions.py::PromptDebugWrapperTests::test_assert_only_wrapper_rejects_generic_tool_label` |
| Prompt-debug wrapper `--assert-only` passes for valid artifacts | `tests/test_auto_ui_assertions.py::PromptDebugWrapperTests::test_assert_only_wrapper_passes_with_valid_report_and_session` |
| Prompt-debug wrapper `--assert-only` fails when `report.json` is missing | `tests/test_auto_ui_assertions.py::PromptDebugWrapperTests::test_assert_only_wrapper_fails_without_report_json` |
| Prompt-debug wrapper fixture pair stays valid for the engineering gate replay path | `contracts/engineering-gate.toml::prompt_debug_plaintext_contract`, `tests/fixtures/prompt_debug_assert/report.json`, `tests/fixtures/prompt_debug_assert/session.json` |
| Plaintext streamed fenced code is promoted into dedicated UI render blocks | `src/app/ui/message.rs::prepare_plaintext_render_blocks_extracts_fenced_code_for_ui_path`, `src/app/ui/message.rs::prepare_plaintext_render_blocks_keeps_transcript_lines_outside_code_block` |
| Plaintext streamed generic code fences normalize to shell highlighting | `src/app/ui/message.rs::prepare_plaintext_render_blocks_normalizes_plaintext_code_language` |
| Full auto-ui suite script parses and advertises the prompt-debug wrapper contract | `tests/test_auto_ui_scripts.py::AutoUiShellScriptTests::test_shell_scripts_parse`, `tests/test_auto_ui_scripts.py::AutoUiShellScriptTests::test_full_suite_mentions_prompt_debug_wrapper_contract` |

### Memory MCP Handler Coverage

MCP tool dispatch, input validation, serialization, and workspace boundary
enforcement. All tests in `src/memory_mcp/tests.rs`.

| Behavior | Named tests |
| --- | --- |
| Workspace memory status routes successfully | `src/memory_mcp/tests.rs::workspace_memory_status_routes_successfully` |
| Status returns error for nonexistent workspace | `src/memory_mcp/tests.rs::workspace_memory_status_returns_error_for_nonexistent_workspace` |
| Status returns error for relative path | `src/memory_mcp/tests.rs::workspace_memory_status_returns_error_for_relative_path` |
| Search memory routes successfully | `src/memory_mcp/tests.rs::search_memory_routes_successfully` |
| Search returns error for nonexistent workspace | `src/memory_mcp/tests.rs::search_memory_returns_error_for_nonexistent_workspace` |
| List invalidations routes successfully | `src/memory_mcp/tests.rs::list_invalidations_routes_successfully` |
| Invalidate doc key routes successfully | `src/memory_mcp/tests.rs::invalidate_doc_key_routes_successfully` |
| Restore doc key routes successfully | `src/memory_mcp/tests.rs::restore_doc_key_routes_successfully` |
| Remember fact routes successfully | `src/memory_mcp/tests.rs::remember_fact_routes_successfully` |
| Remember fact rejects invalid priority | `src/memory_mcp/tests.rs::remember_fact_rejects_invalid_priority` |
| Forget fact routes successfully | `src/memory_mcp/tests.rs::forget_fact_routes_successfully` |
| Forget fact returns changed false for nonexistent memory | `src/memory_mcp/tests.rs::forget_fact_returns_changed_false_for_nonexistent_memory` |
| List manual memories routes successfully | `src/memory_mcp/tests.rs::list_manual_memories_routes_successfully` |
| Service error for workspace outside allowed roots | `src/memory_mcp/tests.rs::service_error_for_workspace_outside_allowed_roots` |
| Service error for relative workspace path | `src/memory_mcp/tests.rs::service_error_for_relative_workspace_path` |
| Service error for invalid priority | `src/memory_mcp/tests.rs::service_error_for_invalid_priority` |
| Status response serializes to valid JSON | `src/memory_mcp/tests.rs::status_response_serializes_to_valid_json` |
| Search response serializes to valid JSON | `src/memory_mcp/tests.rs::search_response_serializes_to_valid_json` |
| Doc key mutation response serializes to valid JSON | `src/memory_mcp/tests.rs::doc_key_mutation_response_serializes_to_valid_json` |
| Remember fact response serializes to valid JSON | `src/memory_mcp/tests.rs::remember_fact_response_serializes_to_valid_json` |
| Manual memories response serializes to valid JSON | `src/memory_mcp/tests.rs::manual_memories_response_serializes_to_valid_json` |
| Invalidations response serializes to valid JSON | `src/memory_mcp/tests.rs::invalidations_response_serializes_to_valid_json` |

### Session MCP Handler Coverage

Session inspect MCP handler dispatch, prefix resolution, and error handling. All
tests in `src/session_mcp/tests/handler_tests.rs`.

| Behavior | Named tests |
| --- | --- |
| Inspect session returns success response | `src/session_mcp/tests/handler_tests.rs::inspect_session_returns_success_response` |
| Inspect session response serializes to valid JSON | `src/session_mcp/tests/handler_tests.rs::inspect_session_response_serializes_to_valid_json` |
| Inspect session returns error for nonexistent session | `src/session_mcp/tests/handler_tests.rs::inspect_session_returns_error_for_nonexistent_session` |
| Inspect session returns error for empty session id | `src/session_mcp/tests/handler_tests.rs::inspect_session_returns_error_for_empty_session_id` |
| Inspect session returns error for ambiguous prefix | `src/session_mcp/tests/handler_tests.rs::inspect_session_returns_error_for_ambiguous_prefix` |
| Inspect session routes by exact id | `src/session_mcp/tests/handler_tests.rs::inspect_session_routes_by_exact_id` |
| Inspect session routes by unique prefix | `src/session_mcp/tests/handler_tests.rs::inspect_session_routes_by_unique_prefix` |
| Inspect session returns hidden flag correctly | `src/session_mcp/tests/handler_tests.rs::inspect_session_returns_hidden_flag_correctly` |
| Session MCP error is plain string not structured JSON | `src/session_mcp/tests/handler_tests.rs::session_mcp_error_is_plain_string_not_structured_json` |
| Session MCP response when serialized has all fields | `src/session_mcp/tests/handler_tests.rs::session_mcp_response_when_serialized_to_json_has_all_fields` |

## chatbot-worker Coverage

`rust-chatbot` now consumes the shared worker/runtime surface from
`../chatbot-worker`. These are the boundary checks that matter most when this
repo changes how it talks to the worker layer.

| Behavior | Named tests |
| --- | --- |
| Workspace memory retrieval | `../chatbot-worker/src/memory/tests/retrieval.rs::retrieve_memory_context_reads_workspace_memmachine` |
| Manual memory priority and retrieval fallback | `../chatbot-worker/src/memory/tests/retrieval.rs::retrieve_memory_context_prefers_manual_memories` |
| Manual memory CRUD round-trip | `../chatbot-worker/src/memory/tests/manual_memory.rs::manual_memories_round_trip_and_forget` |
| Memory invalidation survives rebuild and restore flows | `../chatbot-worker/src/memory/tests/ingest.rs::rebuild_workspace_memory_preserves_invalidated_doc_keys`, `../chatbot-worker/src/memory/tests/retrieval.rs::restore_memory_doc_key_makes_hit_visible_again` |
| Config env override remains the exclusive search path | `../chatbot-worker/src/config/tests/loading.rs::config_path_env_override_is_exclusive_search_entry`, `../chatbot-worker/src/config/tests/loading.rs::config_path_env_override_pointing_at_missing_file_falls_back_to_defaults` |
| Worker state listing skips corrupt entries | `../chatbot-worker/src/worker.rs::list_job_states_returns_valid_entries_alongside_bad_ones` |
| StreamEvent JSONL round-trip and output reconstruction | `../chatbot-worker/src/stream.rs::append_and_read_events_round_trip`, `../chatbot-worker/src/stream.rs::compute_output_concatenates_text_deltas` |
| Worker spool schema versioning accepts legacy v1 artifacts and rejects unknown future versions | `../chatbot-worker/src/worker/tests.rs::worker_request_deserialize_accepts_missing_schema_version_as_v1`, `../chatbot-worker/src/worker/tests.rs::worker_request_deserialize_rejects_unknown_schema_version`, `../chatbot-worker/src/worker/tests.rs::worker_result_deserialize_rejects_unknown_schema_version`, `../chatbot-worker/src/worker/tests.rs::worker_job_snapshot_deserialize_rejects_unknown_schema_version` |
| Background dispatch merges outputs and classifies provider failures | `../chatbot-worker/src/worker_ext/background/tests/mod.rs::test_background_fan_out_merges_all_task_outputs`, `../chatbot-worker/src/worker_ext/background/tests/results.rs::test_provider_failures_become_error_results` |
| runsc prompt rewriting and schema flag wiring remain intact | `../chatbot-worker/src/worker_ext/background/tests/command_building.rs::rewrite_runsc_workspace_paths_rewrites_absolute_workspace_mentions`, `../chatbot-worker/src/worker_ext/background/tests/command_building.rs::build_codex_command_includes_output_schema_flag` |
| Provider bridge config dispatch and CLI error normalization stay stable | `../chatbot-worker/src/provider_bridge/tests/provider_configs.rs::provider_config_dispatch_returns_correct_variant`, `../chatbot-worker/src/provider_bridge/tests/cli_stream.rs::stream_error_includes_command_string` |
| AST search still degrades cleanly to ripgrep or empty results | `../chatbot-worker/src/worker_ext/ast_search.rs::test_execute_uses_ripgrep_fallback_when_ast_grep_missing`, `../chatbot-worker/src/worker_ext/ast_search.rs::test_execute_degrades_when_ast_grep_and_ripgrep_are_missing` |
| Debug-log cleanup and atomic storage primitives stay valid | `../chatbot-worker/src/debug.rs::cleanup_old_logs_keeps_recent_files`, `../chatbot-worker/src/storage/atomic.rs::atomic_write_creates_file_with_content` |
| Shared-memory sync helpers preserve layout and patch semantics | `../chatbot-worker/src/sync/tests.rs::reviewer_and_explorer_encode_as_orchestrator`, `../chatbot-worker/src/sync/tests.rs::apply_patch_updates_tempdir_workspace` |

## auto-runs Coverage

`rust-chatbot` no longer owns autonomous workflow routing, council orchestration,
or code-review planning. The authoritative detailed mapping for those behaviors
now lives in `../auto-runs/docs/verification.md`.

These are the cross-repo contract points that still matter when changing
workflow selection, worker launch expectations, or review/council integrations
from `rust-chatbot`.

| Behavior | Named tests |
| --- | --- |
| Workflow registry keeps the expected core workflows | `../auto-runs/tests/workflow_contracts.rs::workflow_registry_contains_expected_core_workflows` |
| Workflow registry names stay unique and fully populated | `../auto-runs/tests/workflow_contracts.rs::workflow_registry_names_are_unique`, `../auto-runs/tests/workflow_contracts.rs::all_registry_specs_have_required_fields` |
| Workflow specs only reference real context files | `../auto-runs/tests/workflow_contracts.rs::workflow_registry_context_files_exist` |
| DesignImplementation keeps the canonical document pipeline | `../auto-runs/src/coordination/tests/execution_reports_and_restarts.rs::design_implementation_seed_uses_canonical_doc_pipeline` |
| FreeformCouncil remains distinct from the normal DAG path | `../auto-runs/src/coordination/tests/review_schema_and_council_seed.rs::freeform_council_does_not_regress_other_workflow_seeds`, `../auto-runs/src/coordination/tests/review_schema_and_council_seed.rs::freeform_council_seed_returns_no_dag_tasks` |
| Claim adoption still rebinds to the worker PID and rejects mismatched launchers | `../auto-runs/src/coordination/tests/execution_reports_and_restarts.rs::adopting_claim_rebinds_task_to_worker_pid`, `../auto-runs/src/coordination/tests/execution_reports_and_restarts.rs::adopting_claim_rejects_unexpected_launcher_pid` |
| Council turn selection and response parsing remain stable | `../auto-runs/src/coordination/council/tests_selection_and_parsing.rs::mention_driven_turn_prefers_recent_mention`, `../auto-runs/src/coordination/council/tests_selection_and_parsing.rs::parse_and_validate_response_records_claim_delegation_and_vote` |
| Council stall thresholds and claim validation remain enforced | `../auto-runs/src/coordination/council/tests_stall_and_claim_validation.rs::check_stall_soft_nudge_at_three`, `../auto-runs/src/coordination/council/tests_stall_and_claim_validation.rs::validate_claim_rejects_conflicting_scope` |
| Code-review planning and rendering still produce stable artifacts | `../auto-runs/src/code_review/tests_plan_scope_and_render.rs::review_plan_and_metadata_round_trip`, `../auto-runs/src/code_review/tests_chunking_and_prompts.rs::build_review_plan_produces_tasks_with_read_scope` |

## Notes

- `../auto-runs/docs/verification.md` is the detailed source of truth for autonomous workflow coverage.
- `../chatbot-worker` owns the detailed worker/runtime implementation; this document only carries the contract points `rust-chatbot` depends on.
- When adding behavior, update the owning repo's verification artifact first, then add or adjust the cross-repo contract entry here if the change crosses a repo boundary.
The steady-state running-worker poll path is now covered by the focused
release-mode binary tests:

- `app::state::sync_slots_tests::poll_worker_slots_reuses_cached_running_response_when_output_file_is_missing`
- `app::state::sync_slots_tests::build_worker_poll_snapshot_omits_full_running_response_after_offsets_exist`
- `app::state::worker_state_tests::merge_running_response_snapshot_appends_events_without_full_response`
- `app::state::worker_state_tests::update_forge_loop_state_from_cache_uses_cached_response`
- `app::state::sync_slots_tests::poll_worker_slots_done_replaces_stale_provider_session_id`

Together they verify that background chats stay on incremental `events.jsonl`
deltas after offsets are established, stale flat `output` files do not
overwrite the cached transcript, terminal snapshots still replace stale
provider-session identifiers, and Forge loop detection still follows the cached
response text.

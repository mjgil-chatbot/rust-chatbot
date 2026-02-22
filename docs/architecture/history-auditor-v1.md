# History Auditor v1

**Status**: Implemented first production slice; follow-on work remains
**Last updated**: 2026-05-22

This document defines `History Auditor v1` for `rust-chatbot`.

The design goal is not "better search." It is a transcript-first,
evidence-grounded history auditor that can compile reusable agent memory
without overstating certainty.

The key product rule is strict:

```text
Do not claim remote or GitHub truth unless transcript or local repo evidence proves it.
```

That means the auditor should emit verdicts such as:

```text
commit_created_from_log
commit_exists_locally
push_succeeded_from_tool_output
push_failed_non_fast_forward
local_merge_completed
remote_merge_unknown
task_blocked_with_clear_fix
ambiguous_missing_evidence
```

It should not emit "verified on GitHub" or "PR merged" in v1.

## Purpose

`History Auditor` is a transcript-to-memory compiler for coding-agent sessions.

Given a session history, it should answer:

- what was requested
- what the agent intended to do
- what actions were actually attempted
- what succeeded
- what failed
- whether there is a clear next fix
- what future agent memory should be retained
- what evidence supports every conclusion

## Non-goals

V1 explicitly does not include:

- GitHub API verification
- PR API checks
- remote merge proof
- autonomous repo mutation in audit mode
- memory writes without provenance
- trusting final assistant claims over tool evidence

The auditor may inspect a local repo read-only. It should not push, merge,
commit, rebase, fetch, or pull in audit mode.

## Current Rust Dependencies

This design assumes two sibling Rust implementations already exist and should be
used rather than reimplemented inside `rust-chatbot`.

### Semble-rs

The retrieval layer should come from `~/git/semble-rs/semble-rs-work`.

Current structure from that repo's architecture note:

- logical `semble-core`
- logical `semble-cli`
- logical `semble-verification`

Current source ownership in that repo:

- retrieval and ranking in `src/search.rs`, `src/ranking.rs`, `src/index.rs`
- chunking and file traversal in `src/chunking.rs`, `src/file_walker.rs`,
  `src/files.rs`
- public types in `src/types.rs`

For `History Auditor`, `semble-rs` is the evidence retrieval engine. It should
index and retrieve:

- transcript chunks
- shell logs
- JSONL traces
- markdown notes
- config files
- local repo text evidence

It is not responsible for workflow interpretation or verdict generation.

### SemHash-rs

The dedupe, clustering, and representative-sampling layer should come from
`~/git/semhash-rs`.

Current source ownership in that repo:

- public workflow logic in `src/semhash.rs`
- exact similarity/index logic in `src/index.rs`
- result datamodels in `src/datamodels.rs`
- record preparation in `src/records.rs`
- diversification in `src/strategy.rs`

Current design details already documented there matter here:

- stable tie handling in ranking
- exact-search batching in `src/index.rs`
- representative and outlier support already parity-tested against Python

For `History Auditor`, `semhash-rs` is the memory compression layer. It should
be used to:

- dedupe repeated failure narratives
- cluster similar incidents
- identify outlier sessions
- choose representative procedural memories

It is not the evidence store and not the state machine.

## Product Boundary

The ownership split should be:

```text
Semble-rs  = find relevant evidence
SemHash-rs = dedupe, cluster, sample candidate memories
rust-chatbot History Auditor = normalize events, infer task state, assign verdicts, compile memory
```

The missing component is a typed event extractor plus workflow state machine,
not another general search layer.

## Evidence Sources

Without GitHub API verification, the auditor has exactly three evidence
sources:

1. Transcript evidence
   - user requests
   - assistant plans and claims
   - tool calls
   - shell stdout and stderr
   - exit codes
   - test output
   - final assistant summaries
2. Local repo evidence
   - `git status`
   - `git log`
   - local refs and branches
   - local merge ancestry
   - local diffs
3. User-provided artifacts
   - pasted CI logs
   - exported traces
   - text extracted from screenshots
   - imported provider session histories

These sources support local truth and transcript truth only.

## Evidence Language

The output language must stay precise.

Bad:

```text
The PR was merged.
```

Good:

```text
The transcript shows a merge was intended, but no local or transcript evidence proves a remote merge. Verdict: remote_merge_unknown.
```

Good:

```text
git push exited 0 and printed a remote ref update. Verdict: push_succeeded_from_tool_output.
```

Good:

```text
Local branch main contains commit abc123. Verdict: local_merge_completed.
```

## Architecture

```text
                    ┌────────────────────┐
                    │ transcript exports │
                    │ shell logs         │
                    │ local repo path    │
                    │ provider sessions  │
                    └─────────┬──────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────┐
│ 1. Ingestion + normalization                         │
│    Raw transcript → normalized RawEvent records      │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 2. Indexing                                          │
│    Semble index for evidence retrieval               │
│    SemHash index for dedupe/clustering               │
│    SQL event store for provenance                    │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 3. Extraction                                        │
│    rule extractors + structured extraction           │
│    intents, actions, errors, claims, fixes           │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 4. Workflow state machine                            │
│    task state, git state, evidence confidence        │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 5. Audit report                                      │
│    verdict, cause, evidence, missing evidence        │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 6. Memory compiler                                   │
│    episodic, procedural, semantic, anti-pattern      │
└──────────────────────┬───────────────────────────────┘
                       │
                       ▼
┌──────────────────────────────────────────────────────┐
│ 7. MCP tools                                         │
│    retrieve memories and explain prior failures      │
└──────────────────────────────────────────────────────┘
```

## Rust-Chatbot Integration Shape

The implemented ownership has now been extracted into a sibling repository so
the auditor logic and its third-party licensing surface are no longer embedded
directly in `rust-chatbot`.

Current ownership:

```text
history-auditor/
  crates/
    history-auditor-core/
    history-auditor-mcp/

rust-chatbot/
  src/bin/chatbot_history_auditor_mcp.rs
```

The extracted workspace owns:

- transcript ingestion
- typed extraction and verdict logic
- local repo probing
- SQLite persistence
- report generation
- retrieval and memory compilation
- MCP handler and tool schemas

`rust-chatbot` now owns only the thin adapter that loads saved sessions from
its existing session stores, normalizes them into the extracted
`SessionSnapshot` contract, and constructs the extracted MCP handler.

The generic boundary between the product and the extracted repo is:

```text
history-auditor-core
  session.rs
    SessionSnapshot
    InspectedSession
    SessionSource
```

## Core Data Model

The auditor should normalize every transcript turn, tool call, and command
result into typed records.

### RawEvent

Captures the canonical imported event:

- session id
- actor: user, assistant, tool, system, external
- kind: message, tool call, tool result, shell command, shell result, file
  change, error, summary, artifact
- raw content
- tool name and args when present
- command, cwd, stdout, stderr, exit code when present
- source URI and source span
- content hash

### IntentEvent

Captures what was requested or planned:

- commit changes
- push branch
- merge branch
- fix error
- run tests
- edit files
- investigate

Every intent must cite evidence event ids.

### ActionEvent

Captures what was actually attempted:

- `git_status`
- `git_add`
- `git_commit`
- `git_push`
- `git_pull`
- `git_fetch`
- `git_merge`
- `git_rebase`
- `git_checkout`
- `git_log`
- `edit_file`
- `run_tests`

Every action needs:

- status: intended, attempted, succeeded, failed, skipped, unknown
- exit code when available
- stdout or stderr excerpt
- evidence event ids

### ErrorEvent

Captures normalized failure classes:

- `non_fast_forward`
- `auth_failed`
- `protected_branch`
- `pre_receive_hook_declined`
- `remote_not_found`
- `no_upstream_branch`
- `merge_conflict`
- `rebase_conflict`
- `detached_head`
- `nothing_to_commit`
- `dirty_worktree`
- `test_failure`
- `dependency_failure`
- `permission_denied`
- `unknown`

### TaskRun

Captures the high-level session verdict:

- requested goal
- expected actions
- observed action ids
- error ids
- task state
- final verdict
- root cause
- recommended fix
- missing evidence
- confidence

### MemoryRecord

Captures durable outputs for future retrieval:

- episodic
- procedural
- semantic fact
- anti-pattern
- repo fact
- user preference

All memories must retain source task ids and evidence event ids.

## Verdict Taxonomy

The v1 verdict set should be explicit and conservative.

### Commit verdicts

- `commit_not_attempted`
- `commit_created_from_log`
- `commit_exists_locally`

### Push verdicts

- `push_not_attempted`
- `push_succeeded_from_tool_output`
- `push_failed`
- `remote_push_unknown`

### Merge verdicts

- `merge_not_attempted`
- `local_merge_completed`
- `merge_conflict`
- `remote_merge_unknown`

### Task verdicts

- `task_completed`
- `task_blocked_with_clear_fix`
- `task_failed`
- `ambiguous_missing_evidence`

Important state rule:

```text
commit success does not imply push success
local merge success does not imply remote merge success
assistant "Done" does not override failing tool evidence
```

## Confidence Model

Every conclusion should carry both a confidence score and an evidence tier.

### Evidence tiers

Tier 5:

- captured command output at the time of the session
- exit code
- stdout or stderr
- timestamp

Tier 4:

- captured local repo snapshot
- local refs
- local merge ancestry

Tier 3:

- current local repo probe after the fact

Tier 2:

- user or assistant natural-language claim

Tier 1:

- semantic inference without direct evidence

Rule:

```text
Do not emit a completed verdict from Tier 1 or Tier 2 evidence alone.
```

## Local Repo Probe Contract

Audit mode should be read-only and local-only.

Allowed commands:

- `git status --porcelain=v2 --branch`
- `git log --format=<machine-readable-format>`
- `git branch --show-current`
- `git branch -vv`
- `git rev-parse HEAD`
- `git rev-parse --abbrev-ref HEAD`
- `git diff --name-status`
- `git diff --cached --name-status`
- `git merge-base --is-ancestor <a> <b>`

Disallowed in audit mode:

- `git push`
- `git pull`
- `git fetch`
- `git merge`
- `git rebase`
- `git commit`
- `git checkout`
- `git reset`

## Extraction Strategy

The extractor should be hybrid.

### First pass: rules

Use deterministic parsing for:

- git commands
- shell exit codes
- known git error patterns
- test failure patterns
- file edit events

### Second pass: structured extraction

Use model-backed structured extraction only for:

- user intent
- assistant claims
- ambiguous root cause summaries
- reusable memory candidate synthesis

The structured extractor must cite event ids for every field it emits.

If it cannot cite evidence, the field should be `unknown` or omitted.

## Memory Model

The auditor should compile four high-value memory types first.

### Episodic memory

Compact session summary with evidence:

```text
On 2026-05-22 in repo X, the agent was asked to commit and push changes.
It created commit abc123, then push failed with non-fast-forward.
Task ended blocked. Evidence: e17, e18, e19.
```

### Procedural memory

Reusable fix playbook:

```text
When push fails with non-fast-forward on a feature branch:
1. git fetch origin
2. inspect divergence
3. rebase or merge
4. resolve conflicts
5. rerun tests
6. push safely
```

### Semantic fact memory

Durable repo-level fact:

```text
Repo X uses protected main; agents should not push directly to main.
```

### Anti-pattern memory

Behavior to avoid:

```text
Do not mark a task complete merely because a commit was created.
```

## Memory Write Policy

Write durable memory only when:

- there is direct evidence
- the memory is likely reusable
- the scope is clear
- confidence passes threshold
- no secrets are included

Do not write durable memory when:

- the conclusion is only a guess
- the content is one-off noise
- the content contains credentials or private tokens
- the evidence is contradictory
- the task remains ambiguous

Suggested thresholds:

- episodic: `>= 0.60`
- procedural: `>= 0.75`
- repo fact: `>= 0.85`
- anti-pattern: `>= 0.80`
- preference: `>= 0.90` and explicit

## Retrieval Design

Retrieval should be task-shaped, not keyword-only.

Primary retrieval channels:

1. exact filters
   - repo
   - branch
   - task type
   - error type
   - incident pattern
   - session id
   - timestamp
2. `semble-rs` evidence retrieval
   - transcript snippets
   - config files
   - logs
   - repo text evidence
3. `semhash-rs` clustering and dedupe
   - similar failures
   - representative memories
4. graph or relational edges
   - repo → branch → task → error → fix → memory

The auditor should return compact evidence-backed answers, not full transcripts.

## MCP Surface

The v1 MCP surface should stay narrow:

- `audit_session(session_id)`
- `audit_history(query, repo=None, branch=None, task_type=None, session_id=None, final_verdict=None, incident_pattern=None, audited_after=None, audited_before=None)`
- `explain_task(task_id)`
- `find_similar_failures(error_text, repo=None, branch=None, task_type=None, session_id=None, final_verdict=None, incident_pattern=None, audited_after=None, audited_before=None)`
- `get_relevant_memories(query, repo=None, branch=None, max_results=5)`
- `propose_memory_writes(session_id)`
- `approve_memory(memory_id)`
- `retract_memory(memory_id, reason)`

Memory proposal and memory approval should stay separate in v1.

## Storage

Use four stores:

1. raw artifact store
   - transcripts
   - JSONL traces
   - shell logs
2. SQL event store
   - `RawEvent`
   - `IntentEvent`
   - `ActionEvent`
   - `ErrorEvent`
   - `TaskRun`
3. retrieval indexes
   - `semble-rs` for retrieval
   - `semhash-rs` for dedupe and clustering
   - optional SQLite FTS for exact lookup
4. relational graph edges
   - `REQUESTED`
   - `ATTEMPTED`
   - `FAILED_WITH`
   - `FIXED_BY`
   - `PRODUCED_MEMORY`
   - `SIMILAR_TO`
   - `SUPERSEDES`

Do not start with a full temporal graph implementation. A relational edge table
is enough for v1 if evidence and verdicts are correct.

## Current Implementation

The sibling `history-auditor` workspace owns typed ingestion, extraction,
local Git probing, SQLite audit storage, retrieval, and conservative memory
compilation. `rust-chatbot` exposes it through `chatbot-history-auditor-mcp`.
See the [auditor README](../../../history-auditor/README.md) for the current
MCP tool surface and storage layout. The behavioral coverage below defines
its evidence, verdict, retrieval, and memory-promotion boundaries.

## Current Native Enforcement

The extracted implementation is now enforced in the sibling
`history-auditor` repository by:

- `scripts/verify_history_auditor_contract.py`
- `scripts/verify-history-auditor-regressions.sh`

`rust-chatbot` keeps wrapper scripts with the same names that delegate to the
extracted repository so existing operator workflows do not change.

The focused release regression bundle in the extracted repository currently
covers:

- session audit persistence and non-fast-forward verdicting
- public artifact-audit coverage for markdown, JSONL, shell-log, and imported
  provider-session inputs
- memory approval plus retrieval-corpus round trips
- audit-history search and typed task explanation
- branch/task-type constrained audit-history retrieval
- session-id/final-verdict constrained audit-history retrieval
- incident-pattern constrained audit-history and similar-failure retrieval
- timestamp-window constrained audit-history and similar-failure retrieval
- branch-filtered approved-memory retrieval
- branch/session-id/verdict constrained similar-failure retrieval with
  failure-only candidate shaping
- typed SQLite persistence for structured events and relational audit edges
- structured assistant-claim persistence plus evidence-cited task fields
- explicit requested-action gap detection for commit/push/merge/test workflows
- local commit-verdict evidence progression from transcript-only to
  repo-confirmed commit proof, plus ambiguity for unconfirmed commit-only tasks
- push-verdict evidence progression that keeps `git push` `exit 0` transcripts
  ambiguous unless they also include decisive remote-update output
- local merge-verdict evidence progression from weak transcript evidence to
  repo-confirmed `local_merge_completed` via merge-commit ancestry
- ambiguity preservation when push success follows merge or rebase activity
  that never produced decisive success or conflict evidence
- explicit blocking verdict coverage for captured rebase conflicts, plus
  ambiguity coverage for weak rebase-followed-by-push transcripts
- action-backed root-cause fallback and contradiction-aware success blocking
- durable `user_preference` and contradiction-driven `anti_pattern` memory proposals
- repeated-incident `semantic_fact` and `repo_fact` promotion with persisted
  supporting-task provenance and `SIMILAR_TO` audit edges
- approval-time `SUPERSEDES` edges plus weaker-memory retraction when a
  stronger same-kind memory subsumes older guidance
- repeated `non_fast_forward`, `auth_failed`, `remote_not_found`,
  `protected_branch`, `no_upstream_branch`, `detached_head`,
  `nothing_to_commit`, `dirty_worktree`,
  `pre_receive_hook_declined`, and `dependency_failure` memory promotion with
  stronger repeated-support thresholds for both repeated procedural playbooks
  and durable fact memories, plus supporting-task provenance
- inferred `git_delivery_nothing_to_commit` similar-failure shaping from
  commit-shaped query text
- inferred `git_delivery_dirty_worktree` similar-failure shaping from
  broader `working tree dirty` phrasing, plus broader retrieval normalization
  for repository-not-found, upstream-branch, and delivery-shaped
  permission-denied queries
- markdown, JSONL, and shell-log ingestion helpers

## Relationship To Existing Memory Work

This design complements the current MemMachine integration documented in
`docs/architecture/memmachine-integration.md`.

Current MemMachine integration is workspace memory for prompt augmentation.
`History Auditor v1` is a higher-discipline evidence compiler.

The intended relationship is:

- MemMachine remains the broad retrieval store for prior conversation context.
- History Auditor adds typed workflow truth and reusable procedural memory.
- The auditor should write only evidence-backed memories that are safe to
  persist and retrieve later.

The auditor therefore acts as a precision layer above raw transcript retrieval,
not as a replacement for existing workspace memory.

## Implementation Notes For Sibling Repos

### Semble-rs adapter

The `rust-chatbot` adapter around `semble-rs` should expose:

- index transcript and log files with stable source ids
- retrieve scored evidence snippets with file path and line span
- support exact repo-scoped filtering before semantic retrieval
- keep retrieval read-only and deterministic

### SemHash-rs adapter

The `rust-chatbot` adapter around `semhash-rs` should expose:

- dedupe repeated incident summaries
- cluster similar error narratives
- select representative incidents for procedural-memory drafting
- detect outlier sessions that should not become durable defaults

The memory compiler should call `semhash-rs` on normalized incident records,
not on raw full transcripts.

## Canonical Summary

`History Auditor v1` should be built as:

```text
Semble-rs retrieval
+ SemHash-rs dedupe/clustering
+ structured transcript extraction
+ git workflow state machine
+ evidence-ranked verdicts
+ conservative memory compiler
+ MCP retrieval tools
```

No GitHub verifier. No remote claims without proof. Every durable conclusion
must be backed by event ids, evidence spans, and a confidence score.

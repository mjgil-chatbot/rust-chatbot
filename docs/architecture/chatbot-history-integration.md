# Chatbot-History Integration

`rust-chatbot` now integrates the companion `~/git/chatbot-history` workspace as
its local history backend for workspace-scoped audit and checkpoint data.

## Scope

- The integration targets the active chat session inside the selected workspace.
- History data lives under `~/git/.rust-chatbot/ai-history/<workspace-id>/store.db`.
  The workspace ID comes from the shared workspace-memory hashing helper so the
  history store survives workspace-folder cleanup and keeps a stable path.
- A shared backup snapshot lives under
  `~/git/.rust-chatbot/ai-history-backup/<workspace-id>/store.db`. Migrated
  stores restore metadata snapshots plus CAS objects; older backups still
  restore the SQLite store byte-for-byte when no snapshot marker exists.
- The desktop app owns synchronization and read-model projection; it does not
  shell out to the CLI for normal UI interactions.
- For git-backed workspaces, checkpoint payloads are stored as lightweight repo
  metadata + patch artifacts instead of full file-tree snapshots.
- Oversized dirty-worktree diffs or repo patches that would exceed the
  remaining history-CAS quota now degrade to metadata-only checkpoint entries
  instead of failing the history refresh/checkpoint path.
- Repo-patch checkpoint artifacts now write their payload bytes into
  `~/git/.rust-chatbot/ai-history/<workspace-id>/objects/sha256/...` and keep
  the SQLite store as the manifest/index layer for those artifacts.
- Each history refresh also runs app-side maintenance for retained auto
  checkpoints and orphaned repo-patch CAS objects before projecting the latest
  history snapshot into the UI.

## Runtime Flow

1. The UI resolves the effective workspace from the active chat session or the
   workspace picker.
2. `AppState::poll_history_sync()` compares the current chat fingerprint against
   the last synced history request.
3. When the chat has new durable transcript content, the app opens
   `~/git/.rust-chatbot/ai-history/<workspace-id>/store.db` and upserts a
   linked history session using a `rust-chatbot:<chat-id>:<chat-name>` title
   convention.
4. New transcript turns are appended into the `chatbot-history` store's session,
   branch, turn, and message tables.
5. Every finalized conversation turn also creates an automatic workspace
   checkpoint in that same store.
6. After every history mutation, the app writes a metadata-only backup snapshot
   for migrated stores, mirrors any CAS objects under the same workspace
   history root, and records the backup kind alongside that snapshot.
7. If the primary shared history folder is deleted, the next history refresh
   restores either the metadata snapshot + CAS tree or the older exact-copy
   backup, depending on the backup marker present for that workspace.
8. If a repo-patch manifest references a CAS object that is missing from the
   primary history root but still present in the shared backup CAS tree, the
   app restores that single object on demand before loading checkpoint repo
   summaries or replaying the checkpoint.
9. If a checkpoint repo-summary artifact is still unreadable after that
   recovery path, the app skips only that corrupted repo-summary entry and
   keeps the rest of the History panel available.
10. For git-backed workspaces, checkpoint manifests persist:
   - the workspace repo base commit
   - a binary git patch for the current run
   - additional patches for dirty companion repos discovered from Cargo path
     dependencies and the sibling `chatbot-adapter` repo
11. If a dirty repo patch exceeds the bounded repo-patch capture budget or the
    remaining history blob quota, the app keeps the repo metadata summary but
    omits the large patch blob so the history refresh/checkpoint still
    completes.
12. Non-git workspaces still fall back to the existing snapshot scanner path.
13. The app runs history maintenance for the linked workspace root:
    - retain the newest `50` auto-turn checkpoints per linked history session
    - always keep the latest auto checkpoint per branch
    - migrate legacy inline SQLite blob payloads into the CAS in bounded batches
    - only prune/delete history objects after the `24` hour fresh-write grace
      window
    - compute current repo-patch CAS usage and quota-pressure notice state
14. The app loads `BranchTimelineView`, `CheckpointListView`, and `TimelineView`
   models for the linked history session and renders them in the right-side
   History panel.
15. Manual checkpoints use that same manifest builder, then add checkpoint +
   manifest rows.
16. Replay helpers can materialize a checkpoint by cloning each captured repo,
    checking out its recorded base commit, and applying the stored binary patch.
17. If older sync runs left finalized turns without auto-checkpoints, the next
    sync backfills those missing per-turn checkpoints.
18. If a background history refresh or checkpoint worker exits unexpectedly,
    the UI clears its in-flight state and surfaces an error instead of leaving
    panel actions disabled indefinitely.

## UI Surface

- A dedicated right-side `History` panel shows:
  - workspace/store status
  - linked history session information
  - automatic per-turn checkpoint policy
  - git replayable base-commit + patch storage policy
  - shared metadata-snapshot + CAS delete-recovery policy
  - retained auto-checkpoint count
  - repo-patch CAS file count and byte usage
  - maintenance notices, including quota-pressure warnings
  - branch summary
  - checkpoint summary
  - per-checkpoint repo patch summaries with repo role and commit hash
  - explicit "patch omitted" summaries when a dirty repo diff was too large or
    the remaining history blob budget was too small to persist it
  - recent turn timeline
- A checkpoint dialog allows creating named manual checkpoints without leaving
  the desktop app.
- When no store exists yet, the panel can explicitly enable history for the
  current chat/workspace and will restore the deleted primary store from the
  mirrored backup when one exists.

## Operational Maintenance

- `scripts/reset-ai-history.sh` is the repo-local operator helper for clearing
  an oversized primary `~/git/.rust-chatbot/ai-history` root without touching
  `~/git/.rust-chatbot/ai-history-backup`.
- By default the script refuses to run while a `rust-chatbot` desktop process
  is active; `--allow-running-app` is the explicit override for live cleanup.
- The script deletes and recreates only the primary history root, then leaves
  it ready for lazy desktop-app restoration by default.
- Optional restore modes can:
  - restore a selected workspace via `--workspace-root`, using the same
    canonical path + FNV workspace ID mapping as `memory::workspace_id_for_path()`
  - restore a selected workspace directly via `--workspace-id`
  - restore every workspace backup when called with `--restore-all`
- Restored workspace stores keep the current delete-recovery contract because
  the script copies the whole workspace backup directory, including `store.db`,
  `backup-kind`, and any mirrored CAS objects.
- When `sqlite3` is available, the script runs `pragma integrity_check` against
  the restored selected-workspace `store.db` before reporting success.
- If no backup exists for the selected workspace, the script still leaves a
  valid primary `ai-history` root in place so the desktop app can lazily
  recreate the store on the next history refresh.

## File Ownership

- `src/app/state/history.rs`
  - history sync policy
  - shared history store path resolution
  - chat-to-history-session linking
  - per-turn + manual checkpoint creation
  - read-model loading
- `src/app/state/history_sync.rs`
  - existing-turn reconciliation
  - missing auto-checkpoint backfill for partially synced sessions
- `src/app/state/history_maintenance.rs`
  - auto-turn checkpoint retention
  - bounded inline-blob migration
  - orphaned repo-patch CAS object garbage collection
  - repo-patch CAS usage accounting and quota-pressure notices
- `src/app/state/history_repo_capture.rs`
  - git repo discovery
  - repo patch artifact capture
  - repo patch store-backed blob writes + primary/backup CAS fallback reads
  - best-effort repo patch summary loading when individual artifacts are corrupt
  - checkpoint replay materialization helpers
  - manifest/blob construction for checkpoint payloads
- `src/app/state/history_repo_capture_tests.rs`
  - focused repo-patch recovery and corrupted-artifact regressions
  - split out from `history_tests.rs` to keep the history test modules below
    the repo line-count threshold
- `src/app/state/history_support.rs`
  - shared-root path helpers
  - primary/backup store restoration and metadata-snapshot mirroring
  - backup-kind markers
  - history object-store path helpers, atomic object writes, and per-object
    backup CAS restore
  - checkpoint label formatting
  - transcript turn parsing
- `src/app/ui/history_panel.rs`
  - right-side history panel rendering
  - checkpoint dialog rendering
- `src/main.rs`
  - panel/dialog wiring

## Current Limits

- The integration currently syncs the main chat transcript into the history
  store; it does not yet expose restore or branch-fork execution from the UI.
- While a provider response is still streaming, the panel waits for the chat to
  reach a stable state before projecting new durable turn data into
  `chatbot-history`.
- Cargo path-dependency discovery is best-effort; if a workspace `Cargo.toml`
  cannot be parsed, history capture still proceeds for the primary repo and
  skips companion discovery from manifest paths.
- Companion repo capture is limited to Cargo path dependencies plus the sibling
  `chatbot-adapter` checkout; arbitrary sibling repos are not scanned globally.
- Oversized or quota-blocked dirty repo diffs now preserve replay metadata only;
  they no longer abort history refresh, but those checkpoints replay to the
  recorded base commit without the omitted dirty-worktree patch applied.
- Restoring deleted history still depends on the shared backup root remaining
  intact under `~/git/.rust-chatbot/ai-history-backup/...`; deleting both the
  primary history root and the backup root removes the recovery source.
- The current app now bounds new history-store growth with store-native blob
  quota checks, inline-blob migration, checkpoint-count retention,
  orphaned-object GC, and quota-pressure notices.
- Backup snapshots still duplicate CAS payload bytes into the shared backup
  root; the current redesign removes hot-database BLOB growth and full-DB
  mirroring, but not backup-object duplication yet.

## Current Native Enforcement

The current repo-local enforcement surface for the history/checkpoint contract
is:

- `scripts/verify_history_contract.py`
- `scripts/verify-history-regressions.sh`

The verifier keeps this history integration document, the verification mapping,
and the focused release regression bundle aligned. The release bundle exercises
background history-refresh recovery, metadata-snapshot restore from the shared
backup, checkpoint-manifest isolation around invalid Cargo manifests and
unrelated sibling dirtiness, missing auto-checkpoint backfill, auto-checkpoint
retention, orphaned-object GC, and checkpoint repo-summary projection.

The repo-local operator helper for manual oversized-store cleanup is
`scripts/reset-ai-history.sh`.

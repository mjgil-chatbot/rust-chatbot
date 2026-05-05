use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::*;
use crate::app::state::history_repo_capture::{
    build_checkpoint_manifest, materialize_checkpoint_repos,
};
use crate::app::state::history_support::{
    history_object_store_dir, history_store_backup_path, read_history_object, HistoryRestoreKind,
};
use crate::model::ModelProvider;
use ai_history_snapshot::manifest::Manifest;
use ai_history_store::BlobStore;
use rusqlite::Connection;

fn empty_session(id: &str, name: &str) -> ChatSession {
    ChatSession {
        id: id.to_string(),
        name: name.to_string(),
        system_prompt: None,
        json_summary: None,
        claude_session_id: None,
        minimax_session_id: None,
        qwen_session_id: None,
        codex_session_id: None,
        nvidia_session_id: None,
        gemini_session_id: None,
        openrouter_session_id: None,
        omniforge_session_id: None,
        omniforge_model_id: None,
        kimi_session_id: None,
        opencode_session_id: None,
        hermes_session_id: None,
        forge_session_id: None,
        claw_session_id: None,
        launched_from: None,
        runsc_dirs: Vec::new(),
        runsc_image: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: Vec::new(),
    }
}

fn with_temp_home<T>(f: impl FnOnce(&Path) -> T) -> T {
    let _lock = crate::TEST_ENV_LOCK.lock().unwrap();
    let original_home = std::env::var_os("HOME");
    let temp_home = tempfile::tempdir().unwrap();
    let _guard = HomeGuard(original_home);
    std::env::set_var("HOME", temp_home.path());
    f(temp_home.path())
}

fn run_git(repo_root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn init_git_repo(repo_root: &Path, files: &[(&str, &str)]) -> String {
    fs::create_dir_all(repo_root).unwrap();
    run_git(repo_root, &["init", "-q"]);
    run_git(repo_root, &["config", "user.email", "test@example.com"]);
    run_git(repo_root, &["config", "user.name", "Test User"]);
    for (path, contents) in files {
        let file_path = repo_root.join(path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(file_path, contents).unwrap();
    }
    run_git(repo_root, &["add", "."]);
    run_git(repo_root, &["commit", "-qm", "init"]);
    run_git(repo_root, &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

fn read_manifest_entry(
    workspace_root: &Path,
    store: &SqliteStore,
    manifest: &Manifest,
    path: &str,
) -> Vec<u8> {
    let entry = manifest.get_entry(path).unwrap();
    read_history_object(workspace_root, &entry.blob_hash)
        .unwrap()
        .or_else(|| store.read_blob(entry.blob_hash.as_bytes()).unwrap())
        .unwrap()
}

struct HomeGuard(Option<OsString>);

impl Drop for HomeGuard {
    fn drop(&mut self) {
        if let Some(home) = self.0.as_ref() {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}

#[test]
fn linked_history_title_is_stable() {
    let session = empty_session("chat-1", "Demo");
    assert_eq!(
        linked_history_session_title(&session),
        "rust-chatbot:chat-1:Demo"
    );
}

#[test]
fn parse_chat_turns_groups_user_and_assistant_messages() {
    let mut session = empty_session("chat-1", "Demo");
    session.add_message("hello".to_string(), true);
    session.add_message("hi".to_string(), false);
    session.add_message("next".to_string(), true);

    let turns = parse_chat_turns(&session);
    assert_eq!(turns.len(), 2);
    assert_eq!(turns[0].user_content.as_deref(), Some("hello"));
    assert_eq!(turns[0].assistant_content.as_deref(), Some("hi"));
    assert_eq!(turns[1].user_content.as_deref(), Some("next"));
    assert!(turns[1].assistant_content.is_none());
}

#[test]
fn history_request_key_tracks_chat_fingerprint() {
    let mut session = empty_session("chat-1", "Demo");
    session.add_message("hello".to_string(), true);

    let key = history_request_key("/tmp/demo", Some(&session), true);
    assert_eq!(key.workspace_root, "/tmp/demo");
    assert_eq!(key.chat_id.as_deref(), Some("chat-1"));
    assert!(key.chat_fingerprint.is_some());
    assert!(key.force_create);
}

#[test]
fn history_ui_state_defaults_to_empty_snapshot() {
    let state = AppState::new_for_tests(ModelProvider::Codex);
    assert!(state.session.history.snapshot.notice.is_none());
    assert!(state.session.history.snapshot.branch_view.is_none());
}

#[test]
fn poll_history_sync_loads_missing_store_notice_via_background_refresh() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let mut state = AppState::new_for_tests(ModelProvider::Codex);
        state.session.workspace_dir = Some(workspace.to_string_lossy().into_owned());
        state.session.ui_io.repaint_ctx = Some(eframe::egui::Context::default());

        state.poll_history_sync();

        assert!(state.session.history.refresh_in_flight);
        assert_eq!(
            state.session.history.last_request_key,
            Some(history_request_key(
                workspace.to_string_lossy().as_ref(),
                None,
                false
            ))
        );

        for _ in 0..40 {
            state.drain_history_results();
            if !state.session.history.refresh_in_flight {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        assert!(!state.session.history.refresh_in_flight);
        assert_eq!(
            state.session.history.snapshot.notice.as_deref(),
            Some(history_missing_notice().as_str())
        );
    });
}

#[test]
fn drain_history_results_clears_stuck_refresh_on_disconnect() {
    let mut state = AppState::new_for_tests(ModelProvider::Codex);
    let (tx, rx) = mpsc::channel();
    state.session.history.refresh_in_flight = true;
    state.session.history.refresh_rx = Some(rx);
    state.session.history.last_request_key = Some(history_request_key("/tmp/demo", None, false));
    drop(tx);

    state.drain_history_results();

    assert!(!state.session.history.refresh_in_flight);
    assert!(state.session.history.refresh_rx.is_none());
    assert_eq!(
        state.session.history.snapshot.error.as_deref(),
        Some("History refresh stopped unexpectedly.")
    );
}

#[test]
fn drain_history_results_clears_stuck_checkpoint_on_disconnect() {
    let mut state = AppState::new_for_tests(ModelProvider::Codex);
    let (tx, rx) = mpsc::channel();
    state.session.history.checkpoint_in_flight = true;
    state.session.history.checkpoint_rx = Some(rx);
    drop(tx);

    state.drain_history_results();

    assert!(!state.session.history.checkpoint_in_flight);
    assert!(state.session.history.checkpoint_rx.is_none());
    assert_eq!(
        state
            .layout
            .status_feedback
            .as_ref()
            .map(|feedback| feedback.message.as_str()),
        Some("History checkpoint stopped unexpectedly.")
    );
}

#[test]
fn history_store_path_uses_git_shared_root() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        fs::create_dir_all(&workspace).unwrap();

        let workspace_id = crate::memory::workspace_id_for_path(&workspace).unwrap();
        let store_path = history_store_path(&workspace).unwrap();
        let backup_path = history_store_backup_path(&workspace).unwrap();

        assert_eq!(
            store_path,
            home.join("git")
                .join(".rust-chatbot")
                .join("ai-history")
                .join(workspace_id)
                .join("store.db")
        );
        assert_eq!(
            backup_path,
            home.join("git")
                .join(".rust-chatbot")
                .join("ai-history-backup")
                .join(crate::memory::workspace_id_for_path(&workspace).unwrap())
                .join("store.db")
        );
    });
}

#[test]
fn sync_chat_history_creates_auto_checkpoint_per_finalized_turn() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("notes.txt"), "hello").unwrap();

        let mut session = empty_session("chat-1", "Demo");
        session.add_message("first".to_string(), true);
        session.add_message("reply one".to_string(), false);
        session.add_message("second".to_string(), true);
        session.add_message("reply two".to_string(), false);

        let store_path = history_store_path(&workspace).unwrap();
        let backup_path = history_store_backup_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let history_session = sync_chat_history(&mut store, &workspace, &session).unwrap();
        drop(store);
        mirror_history_store(&workspace).unwrap();
        assert!(backup_path.is_file());

        let store = open_history_store(&store_path, false).unwrap();
        let checkpoints = store.list_checkpoints(history_session.id).unwrap();

        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints
            .iter()
            .all(|checkpoint| is_auto_turn_checkpoint_reason(&checkpoint.reason)));
        assert!(checkpoints
            .iter()
            .all(|checkpoint| checkpoint.turn_id.is_some()));
    });
}

#[test]
fn load_history_snapshot_restores_deleted_store_from_metadata_snapshot() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("notes.txt"), "hello").unwrap();

        let mut session = empty_session("chat-1", "Demo");
        session.add_message("first".to_string(), true);
        session.add_message("reply one".to_string(), false);

        let workspace_root = workspace.to_string_lossy().to_string();
        let create_request = HistoryRefreshRequest {
            key: history_request_key(&workspace_root, Some(&session), true),
            chat: Some(session.clone()),
        };
        let initial_snapshot = load_history_snapshot(create_request).unwrap();
        let store_path = PathBuf::from(initial_snapshot.store_path.unwrap());
        let backup_path = history_store_backup_path(&workspace).unwrap();
        assert!(store_path.is_file());
        assert!(backup_path.is_file());

        fs::remove_dir_all(store_path.parent().unwrap()).unwrap();
        assert!(!store_path.exists());

        let restore_request = HistoryRefreshRequest {
            key: history_request_key(&workspace_root, None, false),
            chat: None,
        };
        let restored_snapshot = load_history_snapshot(restore_request).unwrap();
        let expected_notice = history_restored_notice(HistoryRestoreKind::MetadataSnapshot);
        assert!(restored_snapshot.has_store);
        assert_eq!(
            restored_snapshot.notice.as_deref(),
            Some(expected_notice.as_str())
        );
        assert!(restored_snapshot.object_store_files > 0);

        let restored_store = open_history_store(&store_path, false).unwrap();
        let sessions = restored_store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let checkpoints = restored_store.list_checkpoints(sessions[0].id).unwrap();
        assert_eq!(checkpoints.len(), 1);
        assert_eq!(restored_store.quota_report().unwrap().inline_blob_count, 0);
    });
}

#[test]
fn load_history_snapshot_restores_deleted_store_byte_for_byte() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        fs::write(workspace.join("notes.txt"), "hello").unwrap();

        let mut session = empty_session("chat-1", "Demo");
        session.add_message("first".to_string(), true);
        session.add_message("reply one".to_string(), false);

        let store_path = history_store_path(&workspace).unwrap();
        let backup_path = history_store_backup_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        sync_chat_history(&mut store, &workspace, &session).unwrap();
        drop(store);

        fs::create_dir_all(backup_path.parent().unwrap()).unwrap();
        fs::copy(&store_path, &backup_path).unwrap();
        let original_bytes = fs::read(&store_path).unwrap();
        let marker_path = backup_path.parent().unwrap().join("backup-kind");
        if marker_path.exists() {
            fs::remove_file(&marker_path).unwrap();
        }

        fs::remove_dir_all(store_path.parent().unwrap()).unwrap();
        assert!(!store_path.exists());

        let restored = restore_history_store_if_needed(&workspace).unwrap();
        assert_eq!(restored, HistoryRestoreKind::ExactCopy);
        assert_eq!(fs::read(&store_path).unwrap(), original_bytes);
        assert_eq!(fs::read(&store_path).unwrap(), fs::read(&backup_path).unwrap());

        fs::remove_dir_all(store_path.parent().unwrap()).unwrap();
        assert!(!store_path.exists());

        let workspace_root = workspace.to_string_lossy().to_string();
        let restored_snapshot = load_history_snapshot(HistoryRefreshRequest {
            key: history_request_key(&workspace_root, None, false),
            chat: None,
        })
        .unwrap();

        assert!(restored_snapshot.has_store);
        assert_eq!(
            restored_snapshot.notice.as_deref(),
            Some(history_restored_notice(HistoryRestoreKind::ExactCopy).as_str())
        );

        let restored_store = open_history_store(&store_path, false).unwrap();
        let sessions = restored_store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let checkpoints = restored_store.list_checkpoints(sessions[0].id).unwrap();
        assert_eq!(checkpoints.len(), 1);
    });
}

#[test]
fn build_checkpoint_manifest_uses_repo_patch_artifacts_for_git_workspace() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        let head_commit = init_git_repo(
            &workspace,
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"workspace\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", "pub fn answer() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            workspace.join("src/lib.rs"),
            "pub fn answer() -> u32 { 2 }\n",
        )
        .unwrap();
        fs::write(workspace.join("src/new.rs"), "pub fn new_file() {}\n").unwrap();

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();

        assert_eq!(manifest.file_count, 2);
        let metadata_path = "repos/workspace/metadata.json";
        let patch_path = "repos/workspace/working-tree.patch";
        assert!(manifest.get_entry(metadata_path).is_some());
        assert!(manifest.get_entry(patch_path).is_some());

        let metadata: serde_json::Value = serde_json::from_slice(&read_manifest_entry(
            &workspace,
            &store,
            &manifest,
            metadata_path,
        ))
        .unwrap();
        assert_eq!(metadata["base_commit"].as_str(), Some(head_commit.as_str()));
        assert_eq!(
            metadata["apply_commit"].as_str(),
            Some(head_commit.as_str())
        );
        assert_eq!(metadata["role"].as_str(), Some("workspace"));

        let patch = String::from_utf8(read_manifest_entry(
            &workspace, &store, &manifest, patch_path,
        ))
        .unwrap();
        assert!(patch.contains("diff --git a/src/lib.rs b/src/lib.rs"));
        assert!(patch.contains("diff --git a/src/new.rs b/src/new.rs"));

        let object_dir = history_object_store_dir(&workspace).unwrap();
        assert!(object_dir.exists());
        drop(store);

        let conn = Connection::open(&store_path).unwrap();
        let blob_count: i64 = conn
            .query_row("select count(*) from blobs", [], |row| row.get(0))
            .unwrap();
        let blob_object_count: i64 = conn
            .query_row("select count(*) from blob_objects", [], |row| row.get(0))
            .unwrap();
        let inline_blob_bytes: i64 = conn
            .query_row(
                "select coalesce(sum(length(content)), 0) from blobs",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(blob_count, 2);
        assert_eq!(blob_object_count, 2);
        assert_eq!(inline_blob_bytes, 0);
    });
}

#[test]
fn build_checkpoint_manifest_includes_dirty_companion_repo_patches() {
    with_temp_home(|home| {
        let companion = home.join("companion-repo");
        let companion_commit = init_git_repo(
            &companion,
            &[(
                "src/lib.rs",
                "pub fn companion_value() -> &'static str { \"base\" }\n",
            )],
        );
        let workspace = home.join("workspace");
        init_git_repo(
            &workspace,
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"workspace\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\ncompanion = { path = \"../companion-repo\" }\n",
                ),
                ("src/lib.rs", "pub fn answer() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            workspace.join("src/lib.rs"),
            "pub fn answer() -> u32 { 2 }\n",
        )
        .unwrap();
        fs::write(
            companion.join("src/lib.rs"),
            "pub fn companion_value() -> &'static str { \"changed\" }\n",
        )
        .unwrap();

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();

        assert_eq!(manifest.file_count, 4);
        let companion_metadata_path = "repos/companion-repo/metadata.json";
        let companion_patch_path = "repos/companion-repo/working-tree.patch";
        assert!(manifest.get_entry(companion_metadata_path).is_some());
        assert!(manifest.get_entry(companion_patch_path).is_some());

        let metadata: serde_json::Value = serde_json::from_slice(&read_manifest_entry(
            &workspace,
            &store,
            &manifest,
            companion_metadata_path,
        ))
        .unwrap();
        assert_eq!(
            metadata["base_commit"].as_str(),
            Some(companion_commit.as_str())
        );
        assert_eq!(
            metadata["apply_commit"].as_str(),
            Some(companion_commit.as_str())
        );
        assert_eq!(metadata["role"].as_str(), Some("companion"));

        let patch = String::from_utf8(read_manifest_entry(
            &workspace,
            &store,
            &manifest,
            companion_patch_path,
        ))
        .unwrap();
        assert!(patch.contains("diff --git a/src/lib.rs b/src/lib.rs"));
        assert!(patch.contains("changed"));
    });
}

#[test]
fn build_checkpoint_manifest_skips_invalid_cargo_manifest_for_companion_discovery() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        init_git_repo(
            &workspace,
            &[
                ("Cargo.toml", "[package]\nname =\n"),
                ("src/lib.rs", "pub fn answer() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            workspace.join("src/lib.rs"),
            "pub fn answer() -> u32 { 2 }\n",
        )
        .unwrap();

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();

        assert!(manifest
            .get_entry("repos/workspace/metadata.json")
            .is_some());
        assert!(manifest
            .get_entry("repos/workspace/working-tree.patch")
            .is_some());
        assert_eq!(manifest.file_count, 2);
    });
}

#[test]
fn build_checkpoint_manifest_ignores_unrelated_dirty_sibling_repo_patches() {
    with_temp_home(|home| {
        let sibling = home.join("scratch-repo");
        let sibling_commit = init_git_repo(&sibling, &[("notes.txt", "base sibling notes\n")]);
        fs::write(sibling.join("notes.txt"), "dirty sibling notes\n").unwrap();

        let workspace = home.join("workspace");
        init_git_repo(
            &workspace,
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"workspace\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", "pub fn answer() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            workspace.join("src/lib.rs"),
            "pub fn answer() -> u32 { 2 }\n",
        )
        .unwrap();

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();

        let sibling_metadata_path = "repos/scratch-repo/metadata.json";
        let sibling_patch_path = "repos/scratch-repo/working-tree.patch";
        assert!(manifest.get_entry(sibling_metadata_path).is_none());
        assert!(manifest.get_entry(sibling_patch_path).is_none());
        assert_eq!(manifest.file_count, 2);
        assert_eq!(
            run_git(&sibling, &["rev-parse", "HEAD"]).trim(),
            sibling_commit
        );
    });
}

#[test]
fn materialize_checkpoint_repos_replays_workspace_patch() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        let head_commit = init_git_repo(
            &workspace,
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"workspace\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", "pub fn answer() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            workspace.join("src/lib.rs"),
            "pub fn answer() -> u32 { 2 }\n",
        )
        .unwrap();
        fs::write(workspace.join("src/new.rs"), "pub fn new_file() {}\n").unwrap();

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();
        let replay_root = home.join("replayed");

        let materialized =
            materialize_checkpoint_repos(&store, &workspace, &manifest, &replay_root).unwrap();
        assert_eq!(materialized.len(), 1);

        let workspace_repo = &materialized[0];
        assert_eq!(workspace_repo.repo_name, "workspace");
        assert_eq!(workspace_repo.base_commit, head_commit);
        assert_eq!(
            fs::read_to_string(PathBuf::from(&workspace_repo.output_repo_root).join("src/lib.rs"))
                .unwrap(),
            "pub fn answer() -> u32 { 2 }\n"
        );
        assert_eq!(
            fs::read_to_string(PathBuf::from(&workspace_repo.output_repo_root).join("src/new.rs"))
                .unwrap(),
            "pub fn new_file() {}\n"
        );
        assert_eq!(
            run_git(
                Path::new(&workspace_repo.output_repo_root),
                &["rev-parse", "HEAD"],
            )
            .trim(),
            head_commit
        );
    });
}

#[test]
fn sync_chat_history_backfills_missing_auto_turn_checkpoints() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        init_git_repo(
            &workspace,
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"workspace\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", "pub fn answer() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            workspace.join("src/lib.rs"),
            "pub fn answer() -> u32 { 2 }\n",
        )
        .unwrap();

        let mut session = empty_session("chat-1", "Demo");
        session.add_message("first".to_string(), true);
        session.add_message("reply one".to_string(), false);
        session.add_message("second".to_string(), true);
        session.add_message("reply two".to_string(), false);

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let history_session = sync_chat_history(&mut store, &workspace, &session).unwrap();
        drop(store);

        let conn = Connection::open(&store_path).unwrap();
        conn.execute("delete from checkpoints", []).unwrap();
        conn.execute("delete from manifests", []).unwrap();
        drop(conn);

        let mut store = open_history_store(&store_path, false).unwrap();
        sync_chat_history(&mut store, &workspace, &session).unwrap();
        let checkpoints = store.list_checkpoints(history_session.id).unwrap();

        assert_eq!(checkpoints.len(), 2);
        assert!(checkpoints
            .iter()
            .all(|checkpoint| is_auto_turn_checkpoint_reason(&checkpoint.reason)));
    });
}

#[test]
fn load_history_snapshot_exposes_checkpoint_repo_summaries() {
    with_temp_home(|home| {
        let workspace = home.join("workspace");
        init_git_repo(
            &workspace,
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"workspace\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", "pub fn answer() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            workspace.join("src/lib.rs"),
            "pub fn answer() -> u32 { 2 }\n",
        )
        .unwrap();
        fs::write(workspace.join("src/new.rs"), "pub fn new_file() {}\n").unwrap();

        let mut session = empty_session("chat-1", "Demo");
        session.add_message("first".to_string(), true);
        session.add_message("reply one".to_string(), false);

        let workspace_root = workspace.to_string_lossy().to_string();
        let snapshot = load_history_snapshot(HistoryRefreshRequest {
            key: history_request_key(&workspace_root, Some(&session), true),
            chat: Some(session),
        })
        .unwrap();

        assert_eq!(snapshot.checkpoint_repo_summaries.len(), 1);
        let summary = &snapshot.checkpoint_repo_summaries[0];
        assert_eq!(summary.repos.len(), 1);
        assert_eq!(summary.repos[0].repo_name, "workspace");
        assert_eq!(summary.repos[0].role, "workspace");
        assert!(summary.repos[0].patch_bytes > 0);
        assert_eq!(summary.repos[0].untracked_file_count, 1);
    });
}

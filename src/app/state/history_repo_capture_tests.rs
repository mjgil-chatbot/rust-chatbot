use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::super::history_support::{
    history_object_store_backup_dir, history_object_store_dir, history_store_path,
    HistoryRestoreKind,
};
use super::super::*;
use super::*;
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

fn history_object_path(root: &Path, hash_hex: &str) -> PathBuf {
    root.join(&hash_hex[0..2])
        .join(&hash_hex[2..4])
        .join(hash_hex)
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

struct EnvGuard {
    key: &'static str,
    value: Option<OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = env::var_os(key);
        env::set_var(key, value);
        Self {
            key,
            value: previous,
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        if let Some(value) = self.value.as_ref() {
            env::set_var(self.key, value);
        } else {
            env::remove_var(self.key);
        }
    }
}

#[test]
fn restore_history_store_if_needed_restores_repo_patch_objects_from_backup() {
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

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();
        mirror_history_store(&workspace).unwrap();
        drop(store);

        fs::remove_dir_all(store_path.parent().unwrap()).unwrap();
        assert_eq!(
            restore_history_store_if_needed(&workspace).unwrap(),
            HistoryRestoreKind::MetadataSnapshot
        );

        let restored_store = open_history_store(&store_path, false).unwrap();
        let replay_root = home.join("restored-replayed");
        let materialized =
            materialize_checkpoint_repos(&restored_store, &workspace, &manifest, &replay_root)
                .unwrap();

        assert_eq!(materialized.len(), 1);
        assert_eq!(
            fs::read_to_string(PathBuf::from(&materialized[0].output_repo_root).join("src/lib.rs"))
                .unwrap(),
            "pub fn answer() -> u32 { 2 }\n"
        );
    });
}

#[test]
fn materialize_checkpoint_repos_restores_missing_artifact_from_backup_cas() {
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

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();
        mirror_history_store(&workspace).unwrap();

        let metadata_hash = manifest
            .get_entry("repos/workspace/metadata.json")
            .unwrap()
            .blob_hash
            .to_hex();
        let primary_object_path = history_object_path(
            &history_object_store_dir(&workspace).unwrap(),
            &metadata_hash,
        );
        let backup_object_path = history_object_path(
            &history_object_store_backup_dir(&workspace).unwrap(),
            &metadata_hash,
        );
        assert!(primary_object_path.is_file());
        assert!(backup_object_path.is_file());

        let conn = Connection::open(&store_path).unwrap();
        conn.execute(
            "DELETE FROM blob_objects WHERE hash = ?1",
            [metadata_hash.as_str()],
        )
        .unwrap();
        conn.execute(
            "DELETE FROM blobs WHERE hash = ?1",
            [metadata_hash.as_str()],
        )
        .unwrap();
        drop(conn);
        fs::remove_file(&primary_object_path).unwrap();
        assert!(!primary_object_path.exists());

        let replay_root = home.join("replayed-from-backup");
        let materialized =
            materialize_checkpoint_repos(&store, &workspace, &manifest, &replay_root).unwrap();

        assert_eq!(materialized.len(), 1);
        assert!(primary_object_path.is_file());
        assert_eq!(
            fs::read_to_string(PathBuf::from(&materialized[0].output_repo_root).join("src/lib.rs"))
                .unwrap(),
            "pub fn answer() -> u32 { 2 }\n"
        );
    });
}

#[test]
fn load_history_snapshot_skips_corrupted_companion_repo_patch_summaries() {
    with_temp_home(|home| {
        let adapter = home.join("chatbot-adapter");
        init_git_repo(
            &adapter,
            &[
                (
                    "Cargo.toml",
                    "[package]\nname = \"chatbot-adapter\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
                ),
                ("src/lib.rs", "pub fn adapter_value() -> u32 { 1 }\n"),
            ],
        );
        fs::write(
            adapter.join("src/lib.rs"),
            "pub fn adapter_value() -> u32 { 2 }\n",
        )
        .unwrap();

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

        let workspace_root = workspace.to_string_lossy().to_string();
        let initial_snapshot = load_history_snapshot(HistoryRefreshRequest {
            key: history_request_key(&workspace_root, Some(&session), true),
            chat: Some(session.clone()),
        })
        .unwrap();
        assert_eq!(initial_snapshot.checkpoint_repo_summaries.len(), 1);
        assert_eq!(initial_snapshot.checkpoint_repo_summaries[0].repos.len(), 2);

        let store_path = history_store_path(&workspace).unwrap();
        let store = open_history_store(&store_path, false).unwrap();
        let history_session = store.list_sessions().unwrap().pop().unwrap();
        let checkpoint = store
            .list_checkpoints(history_session.id)
            .unwrap()
            .pop()
            .unwrap();
        let manifest_info = store.get_manifest(checkpoint.manifest_id).unwrap().unwrap();
        let manifest: ai_history_snapshot::manifest::Manifest =
            serde_json::from_str(&manifest_info.manifest_data).unwrap();
        let metadata_hash = manifest
            .get_entry("repos/chatbot-adapter/metadata.json")
            .unwrap()
            .blob_hash
            .to_hex();

        let primary_object_path = history_object_path(
            &history_object_store_dir(&workspace).unwrap(),
            &metadata_hash,
        );
        let backup_object_path = history_object_path(
            &history_object_store_backup_dir(&workspace).unwrap(),
            &metadata_hash,
        );
        let conn = Connection::open(&store_path).unwrap();
        conn.execute(
            "DELETE FROM blob_objects WHERE hash = ?1",
            [metadata_hash.as_str()],
        )
        .unwrap();
        conn.execute(
            "DELETE FROM blobs WHERE hash = ?1",
            [metadata_hash.as_str()],
        )
        .unwrap();
        drop(conn);
        fs::remove_file(primary_object_path).unwrap();
        fs::remove_file(backup_object_path).unwrap();

        let recovered_snapshot = load_history_snapshot(HistoryRefreshRequest {
            key: history_request_key(&workspace_root, Some(&session), false),
            chat: Some(session),
        })
        .unwrap();

        assert!(recovered_snapshot.checkpoint_view.is_some());
        assert_eq!(recovered_snapshot.checkpoint_repo_summaries.len(), 1);
        assert_eq!(
            recovered_snapshot.checkpoint_repo_summaries[0].repos.len(),
            1
        );
        assert_eq!(
            recovered_snapshot.checkpoint_repo_summaries[0].repos[0].repo_name,
            "workspace"
        );
    });
}

#[test]
fn build_checkpoint_manifest_omits_oversized_untracked_repo_patch() {
    with_temp_home(|home| {
        let _limit_guard = EnvGuard::set("RUST_CHATBOT_HISTORY_MAX_REPO_PATCH_BYTES", "1024");
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
        fs::write(workspace.join("large.txt"), "x".repeat(2048)).unwrap();

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-23T12:00:00Z").unwrap();

        assert!(manifest
            .get_entry("repos/workspace/metadata.json")
            .is_some());
        assert!(manifest
            .get_entry("repos/workspace/working-tree.patch")
            .is_none());

        let artifact = load_repo_patch_artifact(
            &store,
            &workspace,
            &manifest,
            "repos/workspace/metadata.json",
        )
        .unwrap();
        assert_eq!(
            artifact.metadata.capture_state,
            RepoPatchCaptureState::OmittedOversize
        );
        assert!(artifact.patch.is_empty());
        assert!(artifact.metadata.total_patch_bytes >= 2048);
        assert!(artifact
            .metadata
            .omitted_reason
            .as_deref()
            .unwrap_or_default()
            .contains("repo-patch limit"));
    });
}

#[test]
fn build_checkpoint_manifest_omits_repo_patch_when_remaining_quota_is_too_small() {
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

        let target = RepoTarget {
            role: PRIMARY_REPO_ROLE,
            root: workspace.clone(),
        };
        let preview_artifact = capture_repo_patch(&target, "2026-05-23T12:00:00Z").unwrap();
        let mut omitted_preview = preview_artifact.clone();
        omitted_preview.metadata.capture_state = RepoPatchCaptureState::OmittedQuota;
        omitted_preview.metadata.omitted_reason = Some(
            "repo patch exceeded the remaining history blob budget (18446744073709551615 B available)"
                .to_string(),
        );
        omitted_preview.patch.clear();
        let metadata_budget = serde_json::to_vec_pretty(&omitted_preview.metadata)
            .unwrap()
            .len() as u64;
        let required_bytes = serde_json::to_vec_pretty(&preview_artifact.metadata)
            .unwrap()
            .len() as u64
            + preview_artifact.patch.len() as u64;
        assert!(
            required_bytes > metadata_budget + 64,
            "fixture patch must exceed the metadata-only quota budget"
        );

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        store
            .set_max_total_blob_bytes(metadata_budget + 64)
            .unwrap();
        let manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-23T12:00:00Z").unwrap();

        assert!(manifest
            .get_entry("repos/workspace/metadata.json")
            .is_some());
        assert!(manifest
            .get_entry("repos/workspace/working-tree.patch")
            .is_none());

        let artifact = load_repo_patch_artifact(
            &store,
            &workspace,
            &manifest,
            "repos/workspace/metadata.json",
        )
        .unwrap();
        assert_eq!(
            artifact.metadata.capture_state,
            RepoPatchCaptureState::OmittedQuota
        );
        assert!(artifact.patch.is_empty());
        assert!(artifact.metadata.total_patch_bytes > 0);
        assert!(artifact
            .metadata
            .omitted_reason
            .as_deref()
            .unwrap_or_default()
            .contains("remaining history blob budget"));
    });
}

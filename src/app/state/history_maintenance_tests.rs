use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;

use rusqlite::Connection;

use super::*;
use super::history_support::history_object_store_dir;

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

fn init_git_repo(repo_root: &Path, files: &[(&str, &str)]) {
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

fn touch_file_timestamp(path: &Path, stamp: &str) {
    let output = Command::new("touch")
        .arg("-t")
        .arg(stamp)
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "touch {} failed: {}",
        path.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn touch_recursive(root: &Path, stamp: &str) {
    for entry in fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            touch_recursive(&path, stamp);
        } else {
            touch_file_timestamp(&path, stamp);
        }
    }
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

fn count_files_recursive(root: &Path) -> usize {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .map(|path| {
            if path.is_dir() {
                count_files_recursive(&path)
            } else {
                1
            }
        })
        .sum()
}

#[test]
fn maintain_history_store_prunes_old_auto_checkpoints() {
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
        fs::write(workspace.join("src/lib.rs"), "pub fn answer() -> u32 { 2 }\n").unwrap();

        let mut session = empty_session("chat-1", "Demo");
        for index in 0..55 {
            session.add_message(format!("question {index}"), true);
            session.add_message(format!("answer {index}"), false);
        }

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let history_session = sync_chat_history(&mut store, &workspace, &session).unwrap();
        create_history_checkpoint(
            workspace.to_string_lossy().as_ref(),
            &session,
            "keep-manual",
        )
        .unwrap();
        drop(store);

        let conn = Connection::open(&store_path).unwrap();
        conn.execute(
            "update checkpoints set created_at = '2020-01-01T00:00:00Z' where reason like 'auto-turn:%'",
            [],
        )
        .unwrap();
        drop(conn);

        let mut store = open_history_store(&store_path, false).unwrap();
        let summary = maintain_history_store(&mut store, &workspace).unwrap();
        let checkpoints = store.list_checkpoints(history_session.id).unwrap();
        let auto_count = checkpoints
            .iter()
            .filter(|checkpoint| is_auto_turn_checkpoint_reason(&checkpoint.reason))
            .count();
        let manual_count = checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.reason.starts_with("named:"))
            .count();

        assert_eq!(summary.deleted_auto_checkpoints, 5);
        assert_eq!(summary.auto_checkpoint_count, 50);
        assert_eq!(auto_count, 50);
        assert_eq!(manual_count, 1);
    });
}

#[test]
fn maintain_history_store_deletes_unreferenced_history_objects() {
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
        fs::write(workspace.join("src/lib.rs"), "pub fn answer() -> u32 { 2 }\n").unwrap();
        fs::write(workspace.join("src/new.rs"), "pub fn new_file() {}\n").unwrap();

        let store_path = history_store_path(&workspace).unwrap();
        let mut store = open_history_store(&store_path, true).unwrap();
        let _manifest =
            build_checkpoint_manifest(&mut store, &workspace, "2026-05-19T12:00:00Z").unwrap();

        let object_dir = history_object_store_dir(&workspace).unwrap();
        assert!(object_dir.exists());
        touch_recursive(&object_dir, "202001010000");
        let before_file_count = count_files_recursive(&object_dir);
        assert!(before_file_count > 0);

        let summary = maintain_history_store(&mut store, &workspace).unwrap();
        let after_file_count = count_files_recursive(&object_dir);

        assert!(summary.deleted_object_files > 0);
        assert_eq!(summary.object_store_files, 0);
        assert_eq!(after_file_count, 0);
        assert_eq!(summary.object_store_bytes, 0);
    });
}

#[test]
fn load_history_snapshot_exposes_object_store_usage() {
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
        fs::write(workspace.join("src/lib.rs"), "pub fn answer() -> u32 { 2 }\n").unwrap();
        fs::write(workspace.join("src/new.rs"), "pub fn new_file() {}\n").unwrap();

        let mut session = empty_session("chat-1", "Demo");
        session.add_message("first".to_string(), true);
        session.add_message("reply one".to_string(), false);

        let snapshot = load_history_snapshot(HistoryRefreshRequest {
            key: history_request_key(workspace.to_string_lossy().as_ref(), Some(&session), true),
            chat: Some(session),
        })
        .unwrap();

        assert!(snapshot.object_store_files > 0);
        assert!(snapshot.object_store_bytes > 0);
        assert_eq!(snapshot.auto_checkpoint_count, 1);
    });
}

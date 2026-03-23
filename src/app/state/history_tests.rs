use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use super::*;
use crate::app::state::history_support::read_history_object;
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

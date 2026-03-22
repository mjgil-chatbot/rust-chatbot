use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::process::Command;
use rusqlite::Connection;
use super::*;
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

use super::*;
use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::mpsc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use chatbot_lib::worker::WorkerRequest;
fn make_session(id: &str) -> ChatSession {
    ChatSession {
        id: id.to_string(),
        name: id.to_string(),
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
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        messages: Vec::new(),
        runsc_dirs: Vec::new(),
        runsc_image: None,
    }
}
fn test_app_state(provider: ModelProvider) -> AppState {
    let (_tx, rx) = mpsc::channel();
    AppState {
        session: SessionState::new(provider),
        worker: AppWorkerState::new(),
        layout: LayoutState::new(),
        sync_shm: None,
        sync_socket: SocketHandle::noop(),
        sync_events: rx,
        instance_number: None,
        dialog_parent: None,
    }
}
fn with_temp_home<T>(f: impl FnOnce() -> T) -> T {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home_dir = std::env::temp_dir().join(format!("rust-chatbot-home-{}", uuid::Uuid::new_v4()));
    let old_home = std::env::var_os("HOME");
    std::env::set_var("HOME", &home_dir);

    let result = f();

    match old_home {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    let _ = fs::remove_dir_all(home_dir);
    result
}
fn stale_worker_request(chat_id: &str) -> WorkerRequest {
    WorkerRequest {
        provider: ModelProvider::Claude.as_str().to_string(),
        query: "hello".to_string(),
        working_dir: None,
        system_prompt: None,
        resume_session_id: None,
        memory_context: None,
        parent_pid_override: None,
        runsc_dirs: Vec::new(),
        runsc_image: None,
        model_override: None,
        command: "claude -p hello".to_string(),
        chat_id: chat_id.to_string(),
    }
}
fn set_path_age(path: &Path, age: Duration) {
    let modified = SystemTime::now()
        .checked_sub(age)
        .expect("target modified time should be representable");
    let modified = modified
        .duration_since(UNIX_EPOCH)
        .expect("modified time should be after unix epoch");
    let times = [
        libc::timespec {
            tv_sec: modified.as_secs() as libc::time_t,
            tv_nsec: modified.subsec_nanos() as libc::c_long,
        },
        libc::timespec {
            tv_sec: modified.as_secs() as libc::time_t,
            tv_nsec: modified.subsec_nanos() as libc::c_long,
        },
    ];
    let c_path = CString::new(path.as_os_str().as_bytes()).expect("path bytes");
    let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c_path.as_ptr(), times.as_ptr(), 0) };
    assert_eq!(rc, 0, "utimensat should update {}", path.display());
}

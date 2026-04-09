use super::*;

use std::ffi::CString;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::MutexGuard;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chatbot_lib::worker::WorkerRequest;

fn test_guard() -> MutexGuard<'static, ()> {
    chatbot_lib::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn with_temp_home<T>(f: impl FnOnce() -> T) -> T {
    let _guard = test_guard();
    let home_dir =
        std::env::temp_dir().join(format!("rust-chatbot-ctl-gc-home-{}", uuid::Uuid::new_v4()));
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

#[test]
fn cleanup_gc_targets_removes_stale_request_only_spool_entries() {
    with_temp_home(|| {
        let chat_id = format!("missing-state-{}", uuid::Uuid::new_v4());
        let worker_dir = worker::worker_dir(&chat_id);
        fs::create_dir_all(&worker_dir).expect("create worker dir");
        worker::write_json_atomic(
            &worker_dir.join("request.json"),
            &stale_worker_request(&chat_id),
        )
        .expect("write request");
        set_path_age(&worker_dir.join("request.json"), Duration::from_secs(10));
        set_path_age(&worker_dir, Duration::from_secs(10));

        let cleaned = cleanup_gc_targets();

        assert!(
            cleaned.iter().any(|target| {
                target.chat_id == chat_id
                    && target.state_label == "MISSING_STATE"
                    && target.worker_pid.is_none()
            }),
            "gc cleanup should report the stale request-only spool entry"
        );
        assert!(
            !worker_dir.exists(),
            "gc cleanup should remove the stale request-only spool entry"
        );
    });
}

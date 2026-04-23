use super::*;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;
use tempfile::tempdir;

fn touch(path: &Path) {
    std::fs::write(path, "").unwrap();
}

fn cargo_bin_env_names() -> [String; 2] {
    [
        format!("CARGO_BIN_EXE_{}", crate::config::UI_BINARY_NAME),
        format!(
            "CARGO_BIN_EXE_{}",
            crate::config::UI_BINARY_NAME.replace('-', "_")
        ),
    ]
}

fn with_cargo_bin_env(path: &Path, f: impl FnOnce()) {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let names = cargo_bin_env_names();
    let saved: Vec<_> = names
        .iter()
        .map(|name| (name.clone(), std::env::var_os(name)))
        .collect();
    for name in &names {
        std::env::set_var(name, path);
    }
    f();
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

fn with_cargo_bin_env_cleared(f: impl FnOnce()) {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|err| err.into_inner());
    let names = cargo_bin_env_names();
    let saved: Vec<_> = names
        .iter()
        .map(|name| (name.clone(), std::env::var_os(name)))
        .collect();
    for name in &names {
        std::env::remove_var(name);
    }
    f();
    for (name, value) in saved {
        match value {
            Some(value) => std::env::set_var(name, value),
            None => std::env::remove_var(name),
        }
    }
}

#[test]
fn parse_retrieve_job_requires_workspace_and_query() {
    let job = parse_memory_job(vec![
        "retrieve".to_string(),
        "--workspace-path".to_string(),
        "/tmp/workspace".to_string(),
        "--query".to_string(),
        "hello world".to_string(),
    ])
    .unwrap();

    match job {
        MemoryJob::Retrieve {
            workspace_path,
            query,
        } => {
            assert_eq!(workspace_path, "/tmp/workspace");
            assert_eq!(query, "hello world");
        }
        _ => panic!("expected retrieve job"),
    }
}

#[test]
fn parse_index_chat_job_requires_all_flags() {
    let job = parse_memory_job(vec![
        "index-chat".to_string(),
        "--provider".to_string(),
        "codex".to_string(),
        "--chat-id".to_string(),
        "chat-1".to_string(),
        "--final-state".to_string(),
        "completed".to_string(),
    ])
    .unwrap();

    match job {
        MemoryJob::IndexChat {
            provider,
            chat_id,
            final_state,
        } => {
            assert_eq!(provider, ModelProvider::Codex);
            assert_eq!(chat_id, "chat-1");
            assert_eq!(final_state, "completed");
        }
        _ => panic!("expected index-chat job"),
    }
}

#[test]
fn resolve_memory_job_binary_uses_profile_ui_binary_for_test_hosts() {
    let temp = tempdir().unwrap();
    let profile_dir = temp.path().join("target").join("debug");
    let deps_dir = profile_dir.join("deps");
    std::fs::create_dir_all(&deps_dir).unwrap();

    let ui_binary = profile_dir.join(crate::config::UI_BINARY_NAME);
    let test_host = deps_dir.join("memory_job_tests");
    touch(&ui_binary);
    touch(&test_host);

    assert_eq!(
        resolve_memory_job_binary_from(&test_host).unwrap(),
        ui_binary
    );
}

#[test]
fn resolve_memory_job_binary_errors_when_ui_binary_is_missing() {
    let temp = tempdir().unwrap();
    let profile_dir = temp.path().join("target").join("debug");
    std::fs::create_dir_all(&profile_dir).unwrap();

    let ctl_binary = profile_dir.join("chatbot-ctl");
    touch(&ctl_binary);

    let err = resolve_memory_job_binary_from(&ctl_binary).unwrap_err();
    assert!(err.to_string().contains(crate::config::UI_BINARY_NAME));
}

#[test]
fn resolve_memory_job_target_falls_back_to_in_process_when_ui_binary_is_missing() {
    with_cargo_bin_env_cleared(|| {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("target").join("debug");
        let deps_dir = profile_dir.join("deps");
        std::fs::create_dir_all(&deps_dir).unwrap();

        let test_host = deps_dir.join("memory_job_tests");
        touch(&test_host);

        assert_eq!(resolve_memory_job_target(&test_host).unwrap(), None);
    });
}

#[test]
fn resolve_memory_job_target_errors_outside_test_hosts_when_ui_binary_is_missing() {
    with_cargo_bin_env_cleared(|| {
        let temp = tempdir().unwrap();
        let profile_dir = temp.path().join("target").join("debug");
        std::fs::create_dir_all(&profile_dir).unwrap();

        let ctl_binary = profile_dir.join("chatbot-ctl");
        touch(&ctl_binary);

        assert!(resolve_memory_job_target(&ctl_binary).is_err());
    });
}

#[test]
fn retrieve_memory_context_with_helper_falls_back_when_helper_launch_fails() {
    let temp = tempdir().unwrap();
    let fake_helper = temp.path().join(crate::config::UI_BINARY_NAME);
    touch(&fake_helper);

    with_cargo_bin_env(&fake_helper, || {
        let result =
            retrieve_memory_context_with_helper(temp.path().to_str().unwrap(), "   ").unwrap();
        assert!(result.is_none());
    });
}

#[test]
fn spawn_background_memory_job_falls_back_when_helper_launch_fails() {
    let temp = tempdir().unwrap();
    let fake_helper = temp.path().join(crate::config::UI_BINARY_NAME);
    touch(&fake_helper);
    let fallback_ran = Arc::new(AtomicBool::new(false));

    with_cargo_bin_env(&fake_helper, || {
        let fallback_ran_for_thread = Arc::clone(&fallback_ran);
        spawn_background_memory_job(
            |_| {},
            move || {
                fallback_ran_for_thread.store(true, Ordering::SeqCst);
            },
        )
        .unwrap();

        for _ in 0..20 {
            if fallback_ran.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    });

    assert!(
        fallback_ran.load(Ordering::SeqCst),
        "background helper spawn failure should schedule the in-process fallback"
    );
}

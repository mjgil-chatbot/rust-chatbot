use super::ui_io::SelectedSessionLoad;
use super::*;

use std::ffi::OsString;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::sync::mpsc;
use std::time::Duration as StdDuration;

use tempfile::TempDir;

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

fn make_session_info(id: &str) -> SessionInfo {
    let session = make_session(id);
    SessionInfo {
        id: session.id,
        name: session.name,
        created_at: session.created_at,
        updated_at: session.updated_at,
        message_count: 0,
        hidden: false,
    }
}

struct HomeGuard {
    previous: Option<OsString>,
}

impl HomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", path);
        Self { previous }
    }
}

impl Drop for HomeGuard {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}

fn read_wire_message(stream: &mut UnixStream) -> WireMessage {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).expect("message length");
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).expect("message payload");
    serde_json::from_slice(&payload).expect("wire message json")
}

fn test_app_state(
    provider: ModelProvider,
    sync_socket: SocketHandle,
    sync_events: mpsc::Receiver<SyncEvent>,
) -> AppState {
    AppState {
        session: SessionState::new(provider),
        worker: AppWorkerState::new(),
        layout: LayoutState::new(),
        sync_shm: None,
        sync_socket,
        sync_events,
        instance_number: None,
        dialog_parent: None,
    }
}

#[test]
fn create_session_load_is_applied_and_propagated_immediately() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _guard = HomeGuard::set(temp_home.path());

    let provider = ModelProvider::Claude;
    let (_tx, rx) = mpsc::channel();
    let (writer, mut reader) = UnixStream::pair().expect("socket pair");
    reader
        .set_read_timeout(Some(StdDuration::from_secs(1)))
        .expect("read timeout");

    let mut state = AppState {
        session: SessionState::new(provider),
        worker: AppWorkerState::new(),
        layout: LayoutState::new(),
        sync_shm: Some(crate::sync::SyncShm::open(provider).expect("sync shm")),
        sync_socket: SocketHandle::from_stream(writer),
        sync_events: rx,
        instance_number: None,
        dialog_parent: None,
    };
    state.set_current_session(make_session("current"));
    state.session.selected_session_id = Some("current".to_string());

    let request_id = state.session.ui_io.next_load_request_id();
    state.handle_ui_io_result(UiIoResult::SelectedSessionLoad {
        request_id: Some(request_id),
        requested_session_id: None,
        load: SelectedSessionLoad {
            sessions: Some(vec![
                make_session_info("created"),
                make_session_info("current"),
            ]),
            current_session: Some(make_session("created")),
            requested_session_id: None,
        },
    });

    assert_eq!(state.session.current_session_id(), Some("created"));
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("created")
    );

    let message = read_wire_message(&mut reader);
    assert_eq!(message.msg_type, "sessions");
    assert!(message.chat_id.is_empty());
    assert!(message.provider.is_empty());
    assert!(message.response.is_empty());
    assert!(message.sid.is_none());

    assert!(
        state
            .sync_shm
            .as_mut()
            .expect("sync shm present")
            .sessions_changed(),
        "create-session handling must bump the shared sessions version"
    );
}

#[test]
fn create_session_action_stays_async_even_with_preallocated_session() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _guard = HomeGuard::set(temp_home.path());

    let provider = ModelProvider::Claude;
    let (_tx, rx) = mpsc::channel();
    let (writer, mut reader) = UnixStream::pair().expect("socket pair");
    reader
        .set_read_timeout(Some(StdDuration::from_secs(1)))
        .expect("read timeout");

    let data_manager = DataManager::new(provider).expect("data manager");
    data_manager
        .ensure_preallocated_sessions(1)
        .expect("preallocated session pool");

    let mut state = test_app_state(provider, SocketHandle::from_stream(writer), rx);
    state.session.data_manager = Some(data_manager);
    state.set_current_session(make_session("current"));
    state.session.sessions = vec![make_session_info("current")];

    state.handle_ui_action(AppUiAction::CreateSession {
        name: "Created".to_string(),
        system_prompt: Some("prompt".to_string()),
    });

    assert_eq!(state.session.current_session_id(), Some("current"));
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some("current")
    );
    assert!(state.session.ui_io.is_create_session_in_flight());

    let create_result = state
        .session
        .ui_io
        .rx
        .recv_timeout(StdDuration::from_secs(1))
        .expect("create result");
    state.enqueue_event(AppEvent::UiIo(create_result));
    state.process_event_queue();

    let created = state
        .session
        .current_session
        .clone()
        .expect("checked-out session");
    assert_ne!(created.id, "current");
    assert_eq!(created.name, "Created");
    assert_eq!(created.system_prompt.as_deref(), Some("prompt"));
    assert_eq!(
        state.session.selected_session_id.as_deref(),
        Some(created.id.as_str())
    );
    assert!(!state.session.ui_io.is_create_session_in_flight());
    assert!(state
        .session
        .sessions
        .iter()
        .any(|info| info.id == created.id));

    let stored = state
        .session
        .data_manager
        .as_ref()
        .expect("data manager")
        .load_session(&created.id)
        .expect("stored created session");
    assert_eq!(stored.name, "Created");
    assert_eq!(stored.system_prompt.as_deref(), Some("prompt"));

    let message = read_wire_message(&mut reader);
    assert_eq!(message.msg_type, "sessions");
}

#[test]
fn spawn_create_session_async_selects_locally_and_notifies_other_windows() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _guard = HomeGuard::set(temp_home.path());

    let provider = ModelProvider::Claude;
    let current = make_session("current");
    let data_manager = DataManager::new(provider).expect("data manager");
    data_manager.save_session(&current).expect("save current");

    let (_creator_tx, creator_rx) = mpsc::channel();
    let (_peer_tx, peer_rx) = mpsc::channel();
    let (writer, mut reader) = UnixStream::pair().expect("socket pair");
    reader
        .set_read_timeout(Some(StdDuration::from_secs(1)))
        .expect("read timeout");

    let mut creator = test_app_state(provider, SocketHandle::from_stream(writer), creator_rx);
    creator.set_current_session(current.clone());
    creator.session.sessions = vec![make_session_info("current")];

    let mut peer = test_app_state(provider, SocketHandle::noop(), peer_rx);
    peer.set_current_session(current);
    peer.session.sessions = vec![make_session_info("current")];

    creator.spawn_create_session_async("Created".to_string(), None);

    let create_result = creator
        .session
        .ui_io
        .rx
        .recv_timeout(StdDuration::from_secs(1))
        .expect("create result");
    match &create_result {
        UiIoResult::SelectedSessionLoad {
            request_id: Some(_),
            requested_session_id,
            load,
        } => {
            assert!(requested_session_id.is_none());
            assert!(load.requested_session_id.is_none());
            assert_eq!(
                load.current_session
                    .as_ref()
                    .map(|session| session.name.as_str()),
                Some("Created")
            );
        }
        other => panic!(
            "unexpected create result: {:?}",
            std::mem::discriminant(other)
        ),
    }
    creator.enqueue_event(AppEvent::UiIo(create_result));
    creator.process_event_queue();

    assert_eq!(
        creator
            .session
            .current_session
            .as_ref()
            .map(|session| session.name.as_str()),
        Some("Created")
    );
    let created_session_id = creator
        .session
        .current_session
        .as_ref()
        .map(|session| session.id.clone())
        .expect("created session id");
    assert_eq!(
        creator.session.selected_session_id.as_deref(),
        Some(created_session_id.as_str())
    );

    let message = read_wire_message(&mut reader);
    assert_eq!(message.msg_type, "sessions");

    peer.enqueue_event(AppEvent::Sync(SyncEvent::SessionsChanged));
    peer.process_event_queue();

    let peer_refresh = peer
        .session
        .ui_io
        .rx
        .recv_timeout(StdDuration::from_secs(1))
        .expect("peer refresh result");
    match &peer_refresh {
        UiIoResult::SessionListPatch {
            request_id: Some(_),
            patch,
        } => {
            assert!(patch
                .sessions
                .iter()
                .any(|session| session.id == created_session_id));
        }
        other => panic!(
            "unexpected peer refresh result: {:?}",
            std::mem::discriminant(other)
        ),
    }
    peer.enqueue_event(AppEvent::UiIo(peer_refresh));
    peer.process_event_queue();

    assert_eq!(peer.session.current_session_id(), Some("current"));
    assert!(peer
        .session
        .sessions
        .iter()
        .any(|session| session.id == created_session_id));
}

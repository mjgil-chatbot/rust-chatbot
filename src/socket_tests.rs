use super::*;

use std::ffi::OsString;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::time::Duration;

use tempfile::TempDir;

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

/// Accept a broker client connection, draining any ghost connections
/// left behind by `ensure_broker_best_effort`.
fn accept_broker_client(listener: &UnixListener) -> UnixStream {
    use std::os::fd::AsRawFd;
    loop {
        let (stream, _) = listener.accept().expect("accept client");
        let mut buf = [0u8; 1];
        loop {
            // A closed probe returns EOF. Peek without consuming queued protocol
            // bytes or configuring timeouts on a closed socket (EINVAL on macOS).
            // SAFETY: stream owns the live descriptor and buf is writable for its length.
            let count = unsafe {
                libc::recv(
                    stream.as_raw_fd(),
                    buf.as_mut_ptr().cast(),
                    buf.len(),
                    libc::MSG_PEEK | libc::MSG_DONTWAIT,
                )
            };
            if count == 0 {
                break;
            }
            if count > 0 {
                return stream;
            }
            let error = std::io::Error::last_os_error();
            match error.kind() {
                std::io::ErrorKind::Interrupted => continue,
                std::io::ErrorKind::WouldBlock => return stream,
                std::io::ErrorKind::ConnectionReset => break,
                _ => panic!("peek broker connection: {error}"),
            }
        }
    }
}

#[test]
fn accept_broker_client_preserves_queued_message_with_or_without_closed_probe() {
    for closed_probe in [false, true] {
        let temp = TempDir::new().expect("temporary socket directory");
        let path = temp.path().join("probe.sock");
        let listener = UnixListener::bind(&path).expect("bind listener");
        if closed_probe {
            drop(UnixStream::connect(&path).expect("connect closed probe"));
        }
        let mut client = UnixStream::connect(&path).expect("connect live client");
        let message = WireMessage::sessions_changed();
        let expected = (serde_json::to_vec(&message).unwrap().len() as u32).to_le_bytes();
        write_wire_message(&mut client, &message);
        let mut accepted = accept_broker_client(&listener);
        accepted
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut length = [0u8; 4];
        accepted
            .read_exact(&mut length)
            .expect("read queued frame length");
        assert_eq!(length, expected, "probing must not consume protocol bytes");
    }
}

fn write_wire_message(stream: &mut UnixStream, message: &WireMessage) {
    let payload = serde_json::to_vec(message).expect("serialize wire message");
    let len = (payload.len() as u32).to_le_bytes();
    stream.write_all(&len).expect("write length");
    stream.write_all(&payload).expect("write payload");
}

fn read_wire_message(stream: &mut UnixStream) -> WireMessage {
    let mut len_bytes = [0u8; 4];
    stream.read_exact(&mut len_bytes).expect("read length");
    let len = u32::from_le_bytes(len_bytes) as usize;
    let mut payload = vec![0u8; len];
    stream.read_exact(&mut payload).expect("read payload");
    serde_json::from_slice(&payload).expect("parse wire message")
}

#[test]
fn socket_client_relays_sessions_changed_over_real_broker_socket() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _guard = HomeGuard::set(temp_home.path());

    let sock_path = crate::sync::broker_socket_path();
    std::fs::create_dir_all(
        sock_path
            .parent()
            .expect("broker socket path should have a parent"),
    )
    .expect("create broker socket dir");
    let listener = UnixListener::bind(&sock_path).expect("bind broker socket");

    let (handle, rx) = start(egui::Context::default());
    let mut broker_stream = accept_broker_client(&listener);
    broker_stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set read timeout");

    match rx
        .recv_timeout(Duration::from_secs(1))
        .expect("reconnected")
    {
        SyncEvent::Reconnected => {}
        other => panic!("unexpected initial event: {other:?}"),
    }

    write_wire_message(&mut broker_stream, &WireMessage::sessions_changed());
    match rx
        .recv_timeout(Duration::from_secs(1))
        .expect("sessions changed")
    {
        SyncEvent::SessionsChanged => {}
        other => panic!("unexpected broker event: {other:?}"),
    }

    handle.send(&WireMessage::sessions_changed());
    let outbound = read_wire_message(&mut broker_stream);
    assert_eq!(outbound.msg_type, "sessions");
    assert!(outbound.chat_id.is_empty());
    assert!(outbound.provider.is_empty());
    assert!(outbound.response.is_empty());
    assert!(outbound.sid.is_none());
}

#[test]
fn socket_client_relays_feedback_over_real_broker_socket() {
    let _lock = crate::TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let temp_home = TempDir::new().expect("temp home");
    let _guard = HomeGuard::set(temp_home.path());

    let sock_path = crate::sync::broker_socket_path();
    std::fs::create_dir_all(
        sock_path
            .parent()
            .expect("broker socket path should have a parent"),
    )
    .expect("create broker socket dir");
    let listener = UnixListener::bind(&sock_path).expect("bind broker socket");

    let (handle, rx) = start(egui::Context::default());
    let mut broker_stream = accept_broker_client(&listener);
    broker_stream
        .set_read_timeout(Some(Duration::from_secs(1)))
        .expect("set read timeout");

    match rx
        .recv_timeout(Duration::from_secs(1))
        .expect("reconnected")
    {
        SyncEvent::Reconnected => {}
        other => panic!("unexpected initial event: {other:?}"),
    }

    write_wire_message(
        &mut broker_stream,
        &WireMessage::feedback("chat-1", "claude", "thinking...", true),
    );
    match rx
        .recv_timeout(Duration::from_secs(1))
        .expect("feedback event")
    {
        SyncEvent::StreamFeedback {
            chat_id,
            provider,
            response,
            is_error,
        } => {
            assert_eq!(chat_id, "chat-1");
            assert_eq!(provider, "claude");
            assert_eq!(response, "thinking...");
            assert!(is_error);
        }
        other => panic!("unexpected broker event: {other:?}"),
    }

    handle.send(&WireMessage::feedback(
        "chat-1",
        "claude",
        "tool started",
        false,
    ));
    let outbound = read_wire_message(&mut broker_stream);
    assert_eq!(outbound.msg_type, "feedback");
    assert_eq!(outbound.chat_id, "chat-1");
    assert_eq!(outbound.provider, "claude");
    assert_eq!(outbound.response, "tool started");
    assert_eq!(outbound.is_error, false);
}

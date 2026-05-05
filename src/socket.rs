//! Unix domain socket client for real-time cross-instance sync.
//!
//! Connects to the standalone `chatbot-broker` daemon at
//! `<shared infra>/sync.sock`.  If the broker isn't running yet the
//! reader thread retries with exponential backoff until it appears.
//!
//! Wire format: `[u32 LE length][JSON payload]`

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use rust_debug::tracing::{debug, error, info, warn};
use serde::{Deserialize, Serialize};

// ── protocol messages ───────────────────────────────────────────────

/// Message sent over the wire.
/// Field names in code are readable; `serde(rename)` keeps the JSON compact.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WireMessage {
    /// Message type: "begin", "data", "end", "stopped", "feedback", "sessions"
    #[serde(rename = "t")]
    pub msg_type: String,
    /// Chat ID (empty for "sessions")
    #[serde(rename = "c", default, skip_serializing_if = "String::is_empty")]
    pub chat_id: String,
    /// Provider name (carried in "begin", "data", "end", "stopped")
    #[serde(rename = "p", default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    /// Response text (for "data", "feedback", "end", "stopped")
    #[serde(rename = "r", default, skip_serializing_if = "String::is_empty")]
    pub response: String,
    /// Whether a feedback message indicates an error.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
    /// Session ID (only for "end")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sid: Option<String>,
}

/// Events delivered to the UI thread from the socket reader.
#[derive(Debug)]
pub enum SyncEvent {
    StreamBegin {
        chat_id: String,
        provider: String,
    },
    StreamData {
        chat_id: String,
        provider: String,
        response: String,
    },
    StreamEnd {
        chat_id: String,
        provider: String,
        response: String,
        session_id: Option<String>,
    },
    StreamStopped {
        chat_id: String,
        provider: String,
        response: String,
    },
    StreamFeedback {
        chat_id: String,
        provider: String,
        response: String,
        is_error: bool,
    },
    SessionsChanged,
    Disconnected,
    Reconnected,
}

// ── public handle ───────────────────────────────────────────────────

/// Handle held by the UI thread to send messages and receive events.
#[derive(Clone)]
pub struct SocketHandle {
    writer: Arc<Mutex<Option<UnixStream>>>,
}

impl SocketHandle {
    /// Create a disconnected handle that silently drops all messages.
    /// Used in tests where no broker is running.
    #[cfg(test)]
    pub fn noop() -> Self {
        Self {
            writer: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub fn from_stream(stream: UnixStream) -> Self {
        Self {
            writer: Arc::new(Mutex::new(Some(stream))),
        }
    }

    /// Send a message to all other instances via the broker.
    /// Silently drops the message if not connected.
    pub fn send(&self, msg: &WireMessage) {
        let bytes = match serde_json::to_vec(msg) {
            Ok(b) => b,
            Err(e) => {
                error!(error=%e, "socket: failed to serialize message");
                return;
            }
        };
        let len = bytes.len() as u32;
        let mut guard = match self.writer.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let stream = match guard.as_mut() {
            Some(s) => s,
            None => return, // Not connected — silently drop.
        };
        if stream.write_all(&len.to_le_bytes()).is_err() || stream.write_all(&bytes).is_err() {
            warn!("socket: write failed, clearing connection");
            *guard = None;
        }
    }
}

// ── public entry point ──────────────────────────────────────────────

/// Start the socket client.  Always succeeds — returns immediately.
/// Connection to the broker happens asynchronously in a background thread.
/// If the broker isn't running, the reader thread retries with backoff.
pub fn start(ctx: egui::Context) -> (SocketHandle, mpsc::Receiver<SyncEvent>) {
    let sock_path = sock_path();
    ensure_broker_best_effort(&sock_path);
    let (tx, rx) = mpsc::channel();
    let writer: Arc<Mutex<Option<UnixStream>>> = Arc::new(Mutex::new(None));
    let writer_for_thread = Arc::clone(&writer);

    thread::spawn(move || {
        reader_thread(sock_path, tx, ctx, writer_for_thread);
    });

    let handle = SocketHandle { writer };
    (handle, rx)
}

/// Returns the canonical broker socket path.
pub fn sock_path() -> PathBuf {
    crate::sync::broker_socket_path()
}

fn ensure_broker_best_effort(sock_path: &Path) {
    if UnixStream::connect(sock_path).is_ok() {
        return;
    }

    let broker_bin = match resolve_companion_binary("chatbot-broker") {
        Ok(path) => path,
        Err(err) => {
            warn!(error=%err, "socket: failed to resolve companion broker binary");
            return;
        }
    };

    let mut command = Command::new(&broker_bin);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match command.spawn() {
        Ok(child) => {
            debug!(
                path=%broker_bin.display(),
                pid=child.id(),
                "socket: spawned broker best-effort"
            );
        }
        Err(err) => {
            warn!(
                error=%err,
                path=%broker_bin.display(),
                "socket: failed to spawn broker best-effort"
            );
        }
    }
}

fn current_executable_path() -> std::io::Result<PathBuf> {
    let path = std::env::current_exe()?;
    Ok(path.canonicalize().ok().unwrap_or(path))
}

fn other_profile_dir(current_dir: &Path) -> Option<(&'static str, PathBuf)> {
    let current_profile = current_dir.file_name()?.to_str()?;
    let other_profile = match current_profile {
        "debug" => "release",
        "release" => "debug",
        _ => return None,
    };
    let target_dir = current_dir.parent()?;
    if target_dir.file_name().and_then(|name| name.to_str()) != Some("target") {
        return None;
    }
    Some((other_profile, target_dir.join(other_profile)))
}

fn resolve_companion_binary(binary_name: &str) -> std::io::Result<PathBuf> {
    let current_exe = current_executable_path()?;
    let current_dir = current_exe.parent().unwrap_or(Path::new("."));
    let candidate = current_dir.join(binary_name);
    if candidate.exists() {
        return Ok(candidate);
    }

    if let Some((other_profile, other_dir)) = other_profile_dir(current_dir) {
        let mismatch = other_dir.join(binary_name);
        if mismatch.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "{binary_name} exists only in {other_profile} profile at {}",
                    mismatch.display()
                ),
            ));
        }
    }

    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "{binary_name} was not found alongside {}",
            current_exe.display()
        ),
    ))
}

// ── reader thread with reconnect ────────────────────────────────────

/// Backoff parameters for reconnect attempts.
const BACKOFF_INITIAL_MS: u64 = 100;
const BACKOFF_MAX_MS: u64 = 5_000;
const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;

fn reader_thread(
    sock_path: PathBuf,
    tx: mpsc::Sender<SyncEvent>,
    ctx: egui::Context,
    writer: Arc<Mutex<Option<UnixStream>>>,
) {
    let mut backoff_ms = BACKOFF_INITIAL_MS;
    let mut launch_attempts: u32 = 0;

    loop {
        // ── connect phase ───────────────────────────────────────
        let stream = loop {
            match UnixStream::connect(&sock_path) {
                Ok(s) => {
                    info!(path=%sock_path.display(), "socket: connected to broker");
                    backoff_ms = BACKOFF_INITIAL_MS; // reset on success
                    launch_attempts = 0;
                    break s;
                }
                Err(_) => {
                    if launch_attempts < 3 {
                        ensure_broker_best_effort(&sock_path);
                        launch_attempts += 1;
                    }
                    thread::sleep(Duration::from_millis(backoff_ms));
                    backoff_ms = (backoff_ms * 2).min(BACKOFF_MAX_MS);
                }
            }
        };

        // Install the write half so SocketHandle.send() works.
        let read_stream = match stream.try_clone() {
            Ok(s) => {
                *writer.lock().unwrap() = Some(stream);
                s
            }
            Err(e) => {
                error!(error=%e, "socket: failed to clone stream");
                thread::sleep(Duration::from_millis(backoff_ms));
                backoff_ms = (backoff_ms * 2).min(BACKOFF_MAX_MS);
                continue;
            }
        };

        // Notify UI of (re)connection.
        if tx.send(SyncEvent::Reconnected).is_err() {
            return; // UI dropped — exit thread.
        }
        ctx.request_repaint();

        // ── read phase ──────────────────────────────────────────
        if !read_loop(&read_stream, &tx, &ctx) {
            return; // tx broken, UI is gone.
        }

        // read_loop returned — connection lost.
        *writer.lock().unwrap() = None;
        if tx.send(SyncEvent::Disconnected).is_err() {
            return;
        }
        ctx.request_repaint();
        warn!("socket: disconnected from broker, will reconnect");
    }
}

/// Read messages until an error occurs.
/// Returns `false` if the mpsc sender is broken (UI dropped).
fn read_loop(stream: &UnixStream, tx: &mpsc::Sender<SyncEvent>, ctx: &egui::Context) -> bool {
    let mut stream = stream;
    loop {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).is_err() {
            return true; // connection error — caller will reconnect
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > MAX_MESSAGE_SIZE {
            warn!(len, "socket: message too large from broker");
            return true;
        }

        let mut payload = vec![0u8; len];
        if stream.read_exact(&mut payload).is_err() {
            return true;
        }

        let msg: WireMessage = match serde_json::from_slice(&payload) {
            Ok(m) => m,
            Err(e) => {
                debug!(error=%e, "socket: failed to parse message");
                continue;
            }
        };

        let event = match msg.msg_type.as_str() {
            "begin" => SyncEvent::StreamBegin {
                chat_id: msg.chat_id,
                provider: msg.provider,
            },
            "data" => SyncEvent::StreamData {
                chat_id: msg.chat_id,
                provider: msg.provider,
                response: msg.response,
            },
            "end" => SyncEvent::StreamEnd {
                chat_id: msg.chat_id,
                provider: msg.provider,
                response: msg.response,
                session_id: msg.sid,
            },
            "stopped" => SyncEvent::StreamStopped {
                chat_id: msg.chat_id,
                provider: msg.provider,
                response: msg.response,
            },
            "feedback" => SyncEvent::StreamFeedback {
                chat_id: msg.chat_id,
                provider: msg.provider,
                response: msg.response,
                is_error: msg.is_error,
            },
            "sessions" => SyncEvent::SessionsChanged,
            other => {
                debug!(msg_type = other, "socket: unknown message type");
                continue;
            }
        };

        if tx.send(event).is_err() {
            debug!("socket: UI receiver dropped, exiting reader");
            return false; // UI gone — stop entirely.
        }
        ctx.request_repaint();
    }
}

// ── convenience constructors for WireMessage ────────────────────────

impl WireMessage {
    pub fn begin(chat_id: &str, provider: &str) -> Self {
        Self {
            msg_type: "begin".into(),
            chat_id: chat_id.into(),
            provider: provider.into(),
            response: String::new(),
            is_error: false,
            sid: None,
        }
    }
    pub fn data(chat_id: &str, provider: &str, response: &str) -> Self {
        Self {
            msg_type: "data".into(),
            chat_id: chat_id.into(),
            provider: provider.into(),
            response: response.into(),
            is_error: false,
            sid: None,
        }
    }
    pub fn end(chat_id: &str, provider: &str, response: &str, session_id: Option<&str>) -> Self {
        Self {
            msg_type: "end".into(),
            chat_id: chat_id.into(),
            provider: provider.into(),
            response: response.into(),
            is_error: false,
            sid: session_id.map(|s| s.into()),
        }
    }
    pub fn stopped(chat_id: &str, provider: &str, response: &str) -> Self {
        Self {
            msg_type: "stopped".into(),
            chat_id: chat_id.into(),
            provider: provider.into(),
            response: response.into(),
            is_error: false,
            sid: None,
        }
    }
    pub fn feedback(chat_id: &str, provider: &str, response: &str, is_error: bool) -> Self {
        Self {
            msg_type: "feedback".into(),
            chat_id: chat_id.into(),
            provider: provider.into(),
            response: response.into(),
            is_error,
            sid: None,
        }
    }
    pub fn sessions_changed() -> Self {
        Self {
            msg_type: "sessions".into(),
            chat_id: String::new(),
            provider: String::new(),
            response: String::new(),
            is_error: false,
            sid: None,
        }
    }
}

#[cfg(test)]
#[path = "socket_tests.rs"]
mod tests;

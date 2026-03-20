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

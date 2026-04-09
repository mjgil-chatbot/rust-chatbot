//! chatbot-ctl — management CLI for rust-chatbot instances and broker.
//!
//! Reads shared memory for live UI instances and the durable worker spool
//! for job execution state.
//! Manages the chatbot-broker daemon via systemd user services.
//!
//! Usage:
//!   chatbot-ctl status              Show broker + instances + stream state
//!   chatbot-ctl launch             Launch a UI instance
//!   chatbot-ctl send               Append a user message and start a worker
//!   chatbot-ctl restart-running    Restart currently running UI instances
//!   chatbot-ctl stop [PID...]       Stop UI instances (all if no PIDs given)
//!   chatbot-ctl pids                Print live PIDs only (for scripting)
//!   chatbot-ctl install-desktop     Write .desktop entries
//!   chatbot-ctl uninstall-desktop   Remove .desktop entries
//!   chatbot-ctl install-broker      Write systemd user unit + enable
//!   chatbot-ctl uninstall-broker    Disable + remove systemd unit
//!   chatbot-ctl start-broker        Start broker via systemd
//!   chatbot-ctl stop-broker         Stop broker via systemd
//!   chatbot-ctl restart-broker      Restart broker via systemd
//!   chatbot-ctl broker-status       Show broker systemd status
//!   chatbot-ctl broker-logs         Follow broker journal logs
//!   chatbot-ctl stop-all            Stop UI instances + stop broker

use std::fs;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use anyhow::{Context, Result};
use chatbot_lib::model::ModelProvider;
use chatbot_lib::sync::{self, SyncShm};
use chatbot_lib::worker::{self, WorkerJobSnapshot, WorkerRequest, WorkerState};
use rust_debug::tracing::warn;
include!("chatbot_ctl/helpers.rs");
include!("chatbot_ctl/usage.rs");
include!("chatbot_ctl/backup_scheduler.rs");
include!("chatbot_ctl/commands.rs");
#[cfg(test)]
#[path = "chatbot_ctl/commands_gc_tests.rs"]
mod commands_gc_tests;
include!("chatbot_ctl/broker.rs");
include!("chatbot_ctl/messaging_send.rs");
include!("chatbot_ctl/messaging.rs");
include!("chatbot_ctl/desktop.rs");
include!("chatbot_ctl/database.rs");
fn main() {}

//! Compatibility library for `rust-chatbot` binaries.
//!
//! Shared worker/runtime code now lives in the sibling `chatbot-worker` repo.
//! This crate re-exports that shared surface and keeps the remaining
//! app-specific MCP and background-job modules local.

pub use chatbot_worker_core::{
    chatbot_worker, config, data, debug, ensure_path_symlink, hooks, memory, model, omniforge,
    provider_bridge, provider_catalog, storage, stream, summary, sync, worker, worker_ext,
    worker_process, workspace, AgentRole, AutonomousExecutionEnvironment, TEST_ENV_LOCK,
};
pub mod memory_audit;
pub mod memory_job;

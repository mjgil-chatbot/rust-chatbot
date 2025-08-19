//! Compatibility library for `rust-chatbot` binaries.
//!
//! Shared worker/runtime code now lives in the sibling `chatbot-worker` repo.
//! This crate re-exports that shared surface and keeps the remaining
//! app-specific MCP and background-job modules local.


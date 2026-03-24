//! Desktop Chat Application
//!
//! A desktop GUI application for chatting with multiple model CLIs.
//!
//! ## Architecture
//!
//! The application is organized into the following modules:
//! - `app` - Application state and UI components
//!   - `state` - Main AppState struct and business logic
//!   - `ui` - User interface rendering
//! - `provider_bridge` - Shared provider launch/runtime helpers
//! - `worker` - Detached worker lifecycle and spool integration
//! - `config` - Configuration constants
//! - `data` - Data models and persistence
//! - `markdown` - Markdown rendering with table support

use std::sync::Arc;
use eframe::egui;
pub use chatbot_lib::config;
pub use chatbot_lib::data;
pub use chatbot_lib::memory;
pub use chatbot_lib::memory_job;
pub use chatbot_lib::model;
pub use chatbot_lib::provider_bridge;
pub use chatbot_lib::stream;
pub use chatbot_lib::summary;
pub use chatbot_lib::sync;
pub use chatbot_lib::worker;
pub use chatbot_lib::workspace;
#[cfg(test)]
pub(crate) use chatbot_lib::TEST_ENV_LOCK;
mod app;
mod forge_loop;
mod markdown;
mod socket;
use model::ModelProvider;
use rust_debug::tracing::{error, info};
fn main() {}

//! UI components for the application
//!
//! This module contains all the UI rendering code, organized by component:
//! - `main_panel` - The main chat interface
//! - `side_panel` - Session list and management
//! - `dialogs` - Popup dialogs for session creation/renaming
//! - `message` - Individual message rendering

pub mod dialogs;
pub mod history_panel;
pub(crate) mod layout_log;
pub mod main_panel;
pub mod message;
pub(crate) mod message_list;
mod transcript_summary;

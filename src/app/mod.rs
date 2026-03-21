//! Application module
//!
//! This module contains the application state and UI components.
//!
//! ## Structure
//! - `state` - Application state management (AppState)
//! - `ui` - User interface components
//!   - `dialogs` - Popup dialogs
//!   - `main_panel` - Main chat interface
//!   - `message` - Message rendering
//!   - `side_panel` - Session list

pub mod state;
pub mod ui;
pub(crate) mod workspace_picker;

use chatbot_lib::storage;
use rust_debug::tracing::{info, warn};

// Re-export commonly used types
pub use state::AppState;

pub fn initialize_startup_storage() {
    let report = storage::run_startup_integrity_check();
    let indexed = report.indexed.clone().unwrap_or_default();
    let disabled_reason = storage::index_reads_disabled_reason();

    match report.status {
        storage::IndexIntegrityStatus::Healthy => {
            info!(
                status = report.status.as_str(),
                session_files = report.authoritative.session_files,
                indexed_sessions = indexed.session_index,
                indexed_fts = indexed.session_fts,
                "startup integrity check passed for SQLite read-side index"
            );
        }
        storage::IndexIntegrityStatus::Missing
        | storage::IndexIntegrityStatus::Drifted
        | storage::IndexIntegrityStatus::Unavailable => {
            warn!(
                status=report.status.as_str(),
                read_fallback_forced=report.read_fallback_forced,
                reason=disabled_reason.as_deref().unwrap_or("startup integrity check disabled SQLite reads"),
                session_files=report.authoritative.session_files,
                indexed_sessions=indexed.session_index,
                indexed_fts=indexed.session_fts,
                warning_count=report.warnings.len(),
                warnings=?report.warnings,
                "SQLite read-side index unavailable or stale at startup; authoritative file fallbacks remain active"
            );
        }
    }
}

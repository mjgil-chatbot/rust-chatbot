//! Layout debug logging — writes detailed layout snapshots to disk.
//!
//! Extracted from main_panel.rs to keep the main panel module focused on rendering.

use std::fmt::Write as _;
use std::sync::mpsc;
use chrono::Utc;
use eframe::egui;
pub(crate) struct LayoutLogCompletion {
    pub(crate) result: Result<std::path::PathBuf, String>,
}

use super::*;
use std::ffi::OsString;
use std::time::Duration as StdDuration;
use tempfile::TempDir;
struct HomeGuard {
    previous: Option<OsString>,
}
impl HomeGuard {
    fn set(path: &std::path::Path) -> Self {
        let previous = std::env::var_os("HOME");
        std::env::set_var("HOME", path);
        Self { previous }
    }}
impl Drop for HomeGuard {
    fn drop(&mut self) {
        match self.previous.as_ref() {
            Some(value) => std::env::set_var("HOME", value),
            None => std::env::remove_var("HOME"),
        }
    }
}
fn test_raw_input() -> egui::RawInput {
    let mut raw_input = egui::RawInput::default();
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(1280.0, 900.0),
    ));
    raw_input
}

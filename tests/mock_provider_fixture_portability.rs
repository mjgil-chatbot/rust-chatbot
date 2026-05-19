//! Verify mock-provider setup without a developer checkout or cached adapter.
//! The nested packages use only std, and every build uses the release profile.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
struct FixtureDir(PathBuf);
impl Drop for FixtureDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cargo_ok(directory: &Path, arguments: &[&str], target: &Path) {
    let output = Command::new("cargo")
        .args(arguments)
        .current_dir(directory)
        .env("CARGO_TARGET_DIR", target)
        .env("CARGO_INCREMENTAL", "0")
        .env_remove("RUSTC_WRAPPER")
        .output()
        .expect("start Cargo for isolated std-only fixture");
    assert!(
        output.status.success(),
        "Cargo failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

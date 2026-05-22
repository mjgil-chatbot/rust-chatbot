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

#[test]
fn mock_adapter_wrapper_builds_and_runs_from_a_fresh_sibling_layout() {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = FixtureDir(std::env::temp_dir().join(format!(
        "chatbot-adapter-fixture-{}-{nonce}",
        std::process::id()
    )));
    let app = fixture.0.join("rust-chatbot");
    let adapter = fixture.0.join("chatbot-adapter");
    let target = fixture
        .0
        .join("target with spaces 'and $CHATBOT_FIXTURE_PATH_COMPONENT");
    fs::create_dir_all(app.join("src")).unwrap();
    fs::create_dir_all(adapter.join("src")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/support/mock_provider_server_scripts.rs"),
        app.join("fixture_support.rs"),
    )
    .unwrap();
    fs::write(
        app.join("Cargo.toml"),
        r#"[package]
name="mock-provider-fixture"
version="0.1.0"
edition="2021"
"#,
    )
    .unwrap();
    fs::write(
        adapter.join("Cargo.toml"),
        r#"[package]
name="agent-adapter-cli"
version="0.1.0"
edition="2021"
"#,
    )
    .unwrap();
    fs::write(
        adapter.join("src/main.rs"),
        r#"fn main() { println!("portable-adapter-fixture"); }
"#,
    )
    .unwrap();
    fs::write(
        app.join("src/lib.rs"),
        r#"
#[cfg(test)]
mod fixture {
    use std::{fs, path::{Path, PathBuf}, process::Command};
    mod scripts { include!("../fixture_support.rs"); }
    #[test]
    fn wrapper_executes_the_newly_built_release_adapter() {
        let wrapper = Path::new(env!("CARGO_MANIFEST_DIR")).join("wrapper.sh");
        scripts::write_fake_agent_adapter_cli(&wrapper);
        let expected = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join("release/agent-adapter-cli");
        assert!(expected.is_file(), "fixture must build the configured release adapter");
        let output = Command::new("bash").arg(&wrapper).arg("--version")
            .env("CHATBOT_FIXTURE_PATH_COMPONENT", "must-not-expand")
            .output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "portable-adapter-fixture");
    }
}
"#,
    )
    .unwrap();
    for directory in [&app, &adapter] {
        cargo_ok(directory, &["generate-lockfile", "--offline"], &target);
    }
    cargo_ok(
        &app,
        &[
            "test",
            "--release",
            "--locked",
            "--offline",
            "--",
            "--test-threads=1",
        ],
        &target,
    );
}

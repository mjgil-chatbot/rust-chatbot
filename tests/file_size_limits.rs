use std::fs;
use std::path::PathBuf;
use std::process::Command;

const MAX_LINES: usize = 800;
// Complete dependency notices must retain their full upstream license texts.
const EXEMPT_FILES: &[&str] = &["Cargo.lock", "THIRD_PARTY_LICENSE"];

#[test]
fn repo_files_stay_under_line_limit() {
    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let output = Command::new("git")
        .args(["ls-files", "--cached", "--others", "--exclude-standard"])
        .current_dir(&repo_root)
        .output()
        .expect("git ls-files should run");
    assert!(
        output.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut oversized = Vec::new();
    for raw_path in String::from_utf8_lossy(&output.stdout).lines() {
        if raw_path.is_empty() || EXEMPT_FILES.contains(&raw_path) {
            continue;
        }

        let path = repo_root.join(raw_path);
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let line_count = contents.lines().count();
        if line_count > MAX_LINES {
            oversized.push(format!("{raw_path} ({line_count} lines)"));
        }
    }

    assert!(
        oversized.is_empty(),
        "files exceeded the {MAX_LINES}-line limit:\n{}",
        oversized.join("\n")
    );
}

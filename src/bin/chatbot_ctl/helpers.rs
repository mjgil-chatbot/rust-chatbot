const SERVICE_NAME: &str = "chatbot-broker";
const BACKUP_SCRIPT_RELATIVE_PATH: &str = "scripts/backup-llm-data.sh";
fn current_executable_path() -> Result<PathBuf> {
    let path = std::env::current_exe().context("failed to resolve current executable")?;
    Ok(path.canonicalize().ok().unwrap_or(path))
}
fn configured_repo_toolchain(repo_root: &Path) -> Option<String> {
    let toolchain_file = repo_root.join("rust-toolchain.toml");
    let content = fs::read_to_string(toolchain_file).ok()?;
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("channel = \""))
        .and_then(|value| value.strip_suffix('"'))
        .map(str::to_string)
}
fn rustup_which(toolchain: &str, binary: &str) -> Option<PathBuf> {
    let output = Command::new("rustup")
        .args(["which", "--toolchain", toolchain, binary])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = String::from_utf8(output.stdout).ok()?;
    let trimmed = path.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(PathBuf::from(trimmed))
    }
}
fn repo_cargo_command(repo_root: &Path) -> Command {
    if let Some(toolchain) = configured_repo_toolchain(repo_root) {
        if let Some(cargo_bin) = rustup_which(&toolchain, "cargo") {
            let mut cmd = Command::new(&cargo_bin);
            if let Some(toolchain_bin) = cargo_bin.parent() {
                let path = std::env::var_os("PATH").unwrap_or_default();
                let joined = if path.is_empty() {
                    toolchain_bin.as_os_str().to_os_string()
                } else {
                    std::env::join_paths([toolchain_bin.as_os_str(), path.as_os_str()].into_iter())
                        .unwrap_or_else(|_| toolchain_bin.as_os_str().to_os_string())
                };
                cmd.env("PATH", joined);

                let toolchain_lib = toolchain_bin
                    .parent()
                    .map(|parent| parent.join("lib"))
                    .filter(|path| path.is_dir());
                if let Some(toolchain_lib) = toolchain_lib {
                    let existing = std::env::var_os("LD_LIBRARY_PATH").unwrap_or_default();
                    let joined = if existing.is_empty() {
                        toolchain_lib.as_os_str().to_os_string()
                    } else {
                        std::env::join_paths(
                            [toolchain_lib.as_os_str(), existing.as_os_str()].into_iter(),
                        )
                        .unwrap_or_else(|_| toolchain_lib.as_os_str().to_os_string())
                    };
                    cmd.env("LD_LIBRARY_PATH", joined);
                }
            }
            if let Some(rustc_bin) = rustup_which(&toolchain, "rustc") {
                cmd.env("RUSTC", rustc_bin);
            }
            if let Some(rustdoc_bin) = rustup_which(&toolchain, "rustdoc") {
                cmd.env("RUSTDOC", rustdoc_bin);
            }
            return cmd;
        }

        let mut cmd = Command::new("rustup");
        cmd.args(["run", &toolchain, "cargo"]);
        return cmd;
    }

    Command::new("cargo")
}
fn configured_target_dir(repo_root: &Path) -> PathBuf {
    let default_dir = repo_root.join("target");
    let configured = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or(default_dir);
    if configured.is_absolute() {
        configured
    } else {
        repo_root.join(configured)
    }
}
fn configured_release_dir(repo_root: &Path) -> PathBuf {
    configured_target_dir(repo_root).join("release")
}
fn adapter_repo_root(repo_root: &Path) -> PathBuf {
    std::env::var_os("RUST_CHATBOT_ADAPTER_ROOT")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            repo_root
                .parent()
                .unwrap_or(repo_root)
                .join("chatbot-adapter")
        })
}
fn adapter_target_dir(repo_root: &Path) -> PathBuf {
    std::env::var_os("RUST_CHATBOT_ADAPTER_TARGET_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| adapter_repo_root(repo_root).join("target"))
}
fn adapter_release_binary_path(repo_root: &Path) -> PathBuf {
    adapter_target_dir(repo_root)
        .join("release")
        .join("agent-adapter-cli")
}
fn build_release_with_manifest(
    repo_root: &Path,
    manifest_path: &Path,
    extra_args: &[&str],
) -> Result<()> {
    let mut cmd = repo_cargo_command(repo_root);
    cmd.env("CARGO_TARGET_DIR", configured_target_dir(repo_root))
        .args(["build", "--release", "--quiet", "--manifest-path"])
        .arg(manifest_path)
        .args(extra_args)
        .current_dir(repo_root);
    chatbot_lib::cgroup::wrap_command(&mut cmd);
    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        anyhow::bail!("cargo build failed for {}", manifest_path.display());
    }
    Ok(())
}
fn build_release_ui_stack(repo_root: &Path) -> Result<()> {
    build_release_with_manifest(
        repo_root,
        &repo_root.join("Cargo.toml"),
        &[
            "--bin",
            chatbot_lib::config::UI_BINARY_NAME,
            "--bin",
            "chatbot-ctl",
            "--bin",
            "rcc",
            "--bin",
            "chatbot-worker",
        ],
    )?;

    let adapter_root = adapter_repo_root(repo_root);
    let adapter_manifest = adapter_root.join("Cargo.toml");
    if !adapter_manifest.is_file() {
        anyhow::bail!(
            "chatbot-adapter repo not found at {}. Set RUST_CHATBOT_ADAPTER_ROOT to override the checkout path.",
            adapter_root.display()
        );
    }

    let mut cmd = repo_cargo_command(&adapter_root);
    cmd.env("CARGO_TARGET_DIR", adapter_target_dir(repo_root))
        .args(["build", "--release", "--quiet", "--manifest-path"])
        .arg(&adapter_manifest)
        .args(["-p", "agent-adapter-cli"])
        .current_dir(&adapter_root);
    chatbot_lib::cgroup::wrap_command(&mut cmd);
    let status = cmd.status().context("failed to run adapter cargo build")?;
    if !status.success() {
        anyhow::bail!("cargo build failed for {}", adapter_manifest.display());
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn process_comm(pid: u32) -> Option<String> {
    let path = format!("/proc/{pid}/comm");
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
#[cfg(target_os = "linux")]
fn process_exe_name(pid: u32) -> Option<String> {
    let path = format!("/proc/{pid}/exe");
    fs::read_link(path)
        .ok()
        .and_then(|path| path.file_name().map(|name| name.to_string_lossy().into_owned()))
}
#[cfg(target_os = "linux")]
fn pid_is_ui_process(pid: u32) -> bool {
    if !sync::is_pid_alive(pid) {
        return false;
    }

    let binary_name = chatbot_lib::config::UI_BINARY_NAME;
    process_comm(pid).as_deref() == Some(binary_name)
        || process_exe_name(pid).as_deref() == Some(binary_name)
}
#[cfg(target_os = "linux")]
fn child_pids(pid: u32) -> Vec<u32> {
    let output = match Command::new("pgrep")
        .arg("-P")
        .arg(pid.to_string())
        .output()
    {
        Ok(output) if output.status.success() => output.stdout,
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&output)
        .lines()
        .filter_map(|line| line.trim().parse::<u32>().ok())
        .collect()
}
#[cfg(target_os = "linux")]
fn resolve_ui_instance_pid(pid: u32) -> Option<u32> {
    if pid_is_ui_process(pid) {
        return Some(pid);
    }

    let mut stack = child_pids(pid);
    let mut seen = std::collections::HashSet::new();

    while let Some(candidate) = stack.pop() {
        if !seen.insert(candidate) {
            continue;
        }
        if pid_is_ui_process(candidate) {
            return Some(candidate);
        }
        stack.extend(child_pids(candidate));
    }

    None
}
#[cfg(not(target_os = "linux"))]
fn resolve_ui_instance_pid(pid: u32) -> Option<u32> {
    Some(pid)
}
fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}
fn backup_script_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Ok(current_exe) = current_executable_path() {
        let current_dir = current_exe.parent().unwrap_or(Path::new("."));
        if let Some(target_dir) = current_dir.parent() {
            if let Some(repo_root) = target_dir.parent() {
                push_unique_path(&mut candidates, repo_root.join(BACKUP_SCRIPT_RELATIVE_PATH));
            }
        }
    }

    push_unique_path(
        &mut candidates,
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(BACKUP_SCRIPT_RELATIVE_PATH),
    );

    if let Ok(cwd) = std::env::current_dir() {
        push_unique_path(&mut candidates, cwd.join(BACKUP_SCRIPT_RELATIVE_PATH));
    }

    candidates
}
fn backup_script_path() -> Result<PathBuf> {
    for candidate in backup_script_candidates() {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    anyhow::bail!(
        "could not locate {} from the current executable or current directory",
        BACKUP_SCRIPT_RELATIVE_PATH
    );
}
fn backup_repo_root() -> Result<PathBuf> {
    let script_path = backup_script_path()?;
    let scripts_dir = script_path
        .parent()
        .context("backup script should live in the scripts directory")?;
    let repo_root = scripts_dir
        .parent()
        .context("scripts directory should have a repo root parent")?;
    Ok(repo_root.to_path_buf())
}
fn pid_path() -> PathBuf {
    chatbot_lib::sync::broker_pid_path()
}
fn systemd_unit_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/systemd/user")
}

const SERVICE_NAME: &str = "chatbot-broker";
const BACKUP_SCRIPT_RELATIVE_PATH: &str = "scripts/backup-llm-data.sh";

// ── helpers ─────────────────────────────────────────────────────────

fn format_timestamp(ts: i64) -> String {
    if ts == 0 {
        return "-".to_string();
    }
    chrono::DateTime::from_timestamp(ts, 0)
        .map(|dt| {
            dt.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|| ts.to_string())
}

fn sock_path() -> PathBuf {
    chatbot_lib::sync::broker_socket_path()
}

fn send_wire_message_value(value: serde_json::Value) -> Result<()> {
    let payload = serde_json::to_vec(&value)?;
    let len = (payload.len() as u32).to_le_bytes();
    let mut stream =
        UnixStream::connect(sock_path()).context("failed to connect to chatbot broker socket")?;
    use std::io::Write as _;
    stream.write_all(&len)?;
    stream.write_all(&payload)?;
    Ok(())
}

fn send_begin(chat_id: &str, provider: &str) -> Result<()> {
    send_wire_message_value(serde_json::json!({
        "t": "begin",
        "c": chat_id,
        "p": provider,
    }))
}

fn send_sessions_changed() -> Result<()> {
    send_wire_message_value(serde_json::json!({ "t": "sessions" }))
}

fn parse_provider_name(value: &str) -> Option<ModelProvider> {
    ModelProvider::from_launch_alias(value)
}

fn parse_provider_flag(value: &str) -> Option<ModelProvider> {
    let Some(stripped) = value.strip_prefix("--") else {
        return None;
    };

    if let Some((flag, value)) = stripped.split_once('=') {
        match flag {
            "provider" | "backend" => ModelProvider::from_launch_alias(value),
            _ => None,
        }
    } else {
        ModelProvider::from_launch_alias(stripped)
    }
}

fn configured_default_provider() -> ModelProvider {
    resolve_default_provider(&chatbot_lib::config::config().provider.default_provider)
}

fn resolve_default_provider(provider_name: &str) -> ModelProvider {
    if let Some(provider) = ModelProvider::from_launch_alias(provider_name) {
        provider
    } else {
        eprintln!(
            "Invalid configured default provider '{}'. Falling back to codex.",
            provider_name
        );
        ModelProvider::Codex
    }
}

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

#[cfg(test)]
mod default_provider_tests {
    use super::*;

    #[test]
    fn resolve_default_provider_accepts_aliases() {
        assert_eq!(resolve_default_provider("MINIMAX"), ModelProvider::MiniMax);
    }

    #[test]
    fn resolve_default_provider_falls_back_to_codex_for_unknown_values() {
        assert_eq!(
            resolve_default_provider("does-not-exist"),
            ModelProvider::Codex
        );
    }

    #[test]
    fn parse_provider_flag_accepts_equals_form() {
        assert_eq!(
            parse_provider_flag("--provider=minimax"),
            Some(ModelProvider::MiniMax)
        );
        assert_eq!(
            parse_provider_flag("--backend=kimi"),
            Some(ModelProvider::Kimi)
        );
        assert_eq!(
            parse_provider_flag("--provider=does-not-exist"),
            None
        );
    }
}

fn other_profile_dir(current_dir: &Path) -> Option<(&'static str, PathBuf)> {
    let current_profile = current_dir.file_name()?.to_str()?;
    let other_profile = match current_profile {
        "debug" => "release",
        "release" => "debug",
        _ => return None,
    };
    let target_dir = current_dir.parent()?;
    if target_dir.file_name().and_then(|name| name.to_str()) != Some("target") {
        return None;
    }
    Some((other_profile, target_dir.join(other_profile)))
}

fn binary_names_label(binary_names: &[&str]) -> String {
    match binary_names {
        [] => "binary".to_string(),
        [name] => format!("`{name}`"),
        _ => format!(
            "one of {}",
            binary_names
                .iter()
                .map(|name| format!("`{name}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn resolve_binary_in_current_profile(binary_names: &[&str]) -> Result<PathBuf> {
    let current_exe = current_executable_path()?;
    let current_dir = current_exe.parent().unwrap_or(Path::new("."));

    for binary_name in binary_names {
        let candidate = current_dir.join(binary_name);
        if candidate.exists() {
            return Ok(candidate);
        }
    }

    if let Some((other_profile, other_dir)) = other_profile_dir(current_dir) {
        for binary_name in binary_names {
            let mismatch = other_dir.join(binary_name);
            if mismatch.exists() {
                let current_profile = current_dir
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("current");
                anyhow::bail!(
                    "{} was not found in the current {} build directory {} for {}. Found only {} in {} instead; mixed-profile launches are not allowed.",
                    binary_names_label(binary_names),
                    current_profile,
                    current_dir.display(),
                    current_exe.display(),
                    mismatch.display(),
                    other_profile,
                );
            }
        }
    }

    anyhow::bail!(
        "{} was not found alongside the current executable {} in {}. Build the companion binary in the same profile first.",
        binary_names_label(binary_names),
        current_exe.display(),
        current_dir.display(),
    );
}

fn exit_on_binary_resolution_error(err: anyhow::Error) -> PathBuf {
    eprintln!("{err:#}");
    process::exit(1);
}

fn worker_binary_path() -> PathBuf {
    resolve_binary_in_current_profile(&["chatbot-worker"])
        .unwrap_or_else(exit_on_binary_resolution_error)
}

fn provider_reconcile_owner_pid(provider: ModelProvider) -> u32 {
    SyncShm::open_readonly()
        .map(|shm| {
            shm.live_instances()
                .into_iter()
                .filter(|entry| entry.provider.as_str() == provider.as_str())
                .map(|entry| entry.pid)
                .min()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}

fn pid_path() -> PathBuf {
    chatbot_lib::sync::broker_pid_path()
}

fn broker_binary_path() -> PathBuf {
    resolve_binary_in_current_profile(&["chatbot-broker"])
        .unwrap_or_else(exit_on_binary_resolution_error)
}

fn systemd_unit_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".config/systemd/user")
}

fn systemd_unit_path() -> PathBuf {
    systemd_unit_dir().join(format!("{}.service", SERVICE_NAME))
}

/// Check broker health: PID alive + socket connectable.
fn broker_health() -> BrokerHealth {
    let pid_file = pid_path();
    let sock = sock_path();

    let pid = fs::read_to_string(&pid_file)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok());

    let pid_alive = pid.map(|p| sync::is_pid_alive(p)).unwrap_or(false);
    let socket_ok = UnixStream::connect(&sock).is_ok();

    BrokerHealth {
        pid,
        pid_alive,
        socket_ok,
    }
}

struct BrokerHealth {
    pid: Option<u32>,
    pid_alive: bool,
    socket_ok: bool,
}

impl BrokerHealth {
    fn is_running(&self) -> bool {
        self.pid_alive && self.socket_ok
    }
}

#[derive(Debug, Clone)]
struct InstanceSnapshot {
    pid: u32,
    cwd: PathBuf,
    args: Vec<String>,
    window_placement: Option<WindowPlacement>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowPlacement {
    desktop: i32,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl WindowPlacement {
    fn startup_args(self) -> Vec<String> {
        vec![
            "--window-x".to_string(),
            self.x.to_string(),
            "--window-y".to_string(),
            self.y.to_string(),
            "--window-width".to_string(),
            self.width.to_string(),
            "--window-height".to_string(),
            self.height.to_string(),
        ]
    }
}

// ── commands ────────────────────────────────────────────────────────

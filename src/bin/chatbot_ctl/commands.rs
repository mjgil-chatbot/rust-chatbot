fn cmd_status() {
    // ── Broker status ───────────────────────────────────────────
    let health = broker_health();
    println!("=== Broker ===");
    if let Some(pid) = health.pid {
        let status = if health.is_running() {
            "RUNNING"
        } else if health.pid_alive {
            "DEGRADED (socket not connectable)"
        } else {
            "DOWN (process dead)"
        };
        let socket_str = if health.socket_ok { "ok" } else { "fail" };
        println!("  PID {}  {}  (socket: {})", pid, status, socket_str);
    } else {
        let socket_str = if health.socket_ok {
            "ok (no PID file)"
        } else {
            "not running"
        };
        println!("  {}", socket_str);
    }

    // ── Shared memory + instances ───────────────────────────────
    if let Some(shm) = SyncShm::open_readonly() {
        println!("\n=== Shared Memory ===");
        println!("  version  : {}", shm.version());

        let instances = shm.list_instances();
        let live: Vec<_> = instances
            .iter()
            .filter(|i| sync::is_pid_alive(i.pid))
            .collect();
        let dead: Vec<_> = instances
            .iter()
            .filter(|i| !sync::is_pid_alive(i.pid) && i.pid != 0)
            .collect();

        println!(
            "\n=== Instances ({} live, {} stale) ===",
            live.len(),
            dead.len()
        );
        for inst in &instances {
            if inst.pid == 0 {
                continue;
            }
            let alive = sync::is_pid_alive(inst.pid);
            let status = if alive { "RUNNING" } else { "DEAD" };
            println!(
                "  PID {:>7}  {:>8}  {:>7}  started {}",
                inst.pid,
                inst.provider.as_str(),
                status,
                format_timestamp(inst.started_at),
            );
        }

        if live.is_empty() && dead.is_empty() {
            println!("  (none registered)");
        }
    } else {
        println!("\n=== Instances ===\n  (no shared memory — no instances have run yet)");
    }

    // ── Worker status ──────────────────────────────────────────────
    let workers = worker_jobs();
    let active_workers: Vec<&WorkerJobSnapshot> =
        workers.iter().filter(|w| worker_job_running(w)).collect();
    let other_workers: Vec<&WorkerJobSnapshot> =
        workers.iter().filter(|w| !worker_job_running(w)).collect();

    println!(
        "\n=== Workers ({} active, {} other) ===",
        active_workers.len(),
        other_workers.len()
    );
    for w in &workers {
        let status = worker_job_status(w);
        let elapsed = chrono::Utc::now().timestamp() - w.started_at;
        let size_str = format_size(w.output_len);
        let short_id = if w.chat_id.len() > 8 {
            &w.chat_id[..8]
        } else {
            &w.chat_id
        };
        println!(
            "  PID {:>7}  {:>8}  chat:{}  {:>7}  {}s  {}",
            w.worker_pid, w.provider, short_id, status, elapsed, size_str,
        );
    }
    if workers.is_empty() {
        println!("  (none)");
    }
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

fn worker_jobs() -> Vec<WorkerJobSnapshot> {
    let mut jobs = match worker::list_job_states() {
        Ok(jobs) => jobs,
        Err(err) => {
            eprintln!("failed to read worker spool states: {err:#}");
            return Vec::new();
        }
    };
    jobs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    jobs
}

fn worker_job_running(job: &WorkerJobSnapshot) -> bool {
    job.state == WorkerState::Running && sync::is_pid_alive(job.worker_pid)
}

fn worker_job_status(job: &WorkerJobSnapshot) -> &'static str {
    if job.state == WorkerState::Running && !sync::is_pid_alive(job.worker_pid) {
        "DEAD"
    } else {
        job.state.name()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GcCleanupTarget {
    chat_id: String,
    state_label: &'static str,
    worker_pid: Option<u32>,
    dir: PathBuf,
}

impl GcCleanupTarget {
    fn from_job(job: WorkerJobSnapshot) -> Self {
        Self {
            chat_id: job.chat_id.clone(),
            state_label: job.state.name(),
            worker_pid: Some(job.worker_pid),
            dir: worker::worker_dir(&job.chat_id),
        }
    }

    fn missing_state(chat_id: String, dir: PathBuf) -> Self {
        Self {
            chat_id,
            state_label: "MISSING_STATE",
            worker_pid: None,
            dir,
        }
    }
}

fn collect_gc_targets() -> Vec<GcCleanupTarget> {
    let mut targets = collect_missing_state_gc_targets();
    targets.extend(
        worker_jobs()
            .into_iter()
            .filter(worker_job_should_gc)
            .map(GcCleanupTarget::from_job),
    );
    targets
}

fn cleanup_gc_targets() -> Vec<GcCleanupTarget> {
    let targets = collect_gc_targets();
    for target in &targets {
        if target.dir.exists() {
            let _ = fs::remove_dir_all(&target.dir);
        }
    }
    targets
}

fn worker_job_should_gc(job: &WorkerJobSnapshot) -> bool {
    match job.state {
        WorkerState::Running => !sync::is_pid_alive(job.worker_pid),
        WorkerState::Done | WorkerState::Stopped | WorkerState::Error => true,
        WorkerState::Free => false,
    }
}

fn collect_missing_state_gc_targets() -> Vec<GcCleanupTarget> {
    let entries = match fs::read_dir(worker::workers_dir()) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(err) => {
            eprintln!("failed to read worker spool directory: {err:#}");
            return Vec::new();
        }
    };

    let mut targets = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let dir = entry.path();
        if dir.join("state.json").exists() || missing_state_gc_entry_is_transient(&dir) {
            continue;
        }

        let chat_id = read_missing_state_gc_chat_id(&dir).unwrap_or_else(|| {
            entry
                .file_name()
                .to_str()
                .map(str::to_string)
                .unwrap_or_else(|| dir.display().to_string())
        });
        targets.push(GcCleanupTarget::missing_state(chat_id, dir));
    }

    targets.sort_by(|a, b| a.chat_id.cmp(&b.chat_id));
    targets
}

fn missing_state_gc_entry_is_transient(dir: &Path) -> bool {
    const STARTUP_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

    [dir.join("request.json"), dir.to_path_buf()]
        .into_iter()
        .filter_map(|path| path.metadata().ok())
        .filter_map(|metadata| metadata.modified().ok())
        .filter_map(|modified| modified.elapsed().ok())
        .any(|elapsed| elapsed <= STARTUP_GRACE)
}

fn read_missing_state_gc_chat_id(dir: &Path) -> Option<String> {
    let json = fs::read_to_string(dir.join("request.json")).ok()?;
    serde_json::from_str::<WorkerRequest>(&json)
        .ok()
        .map(|request| request.chat_id)
}

fn cmd_pids() {
    let shm = match SyncShm::open_readonly() {
        Some(s) => s,
        None => return,
    };
    for entry in shm.live_instances() {
        println!("{}", entry.pid);
    }
}

fn cmd_stop(pids_to_kill: &[u32]) {
    let shm = match SyncShm::open_readonly() {
        Some(s) => s,
        None => {
            eprintln!("No shared memory region found. Falling back to signal-based stop...");
            if pids_to_kill.is_empty() {
                // No specific PIDs: kill all matching binaries
                for binary in [chatbot_lib::config::UI_BINARY_NAME] {
                    let _ = Command::new("pkill").args(["-x", binary]).status();
                }
            } else {
                // Send SIGTERM to the specific requested PIDs
                for &pid in pids_to_kill {
                    unsafe {
                        libc::kill(pid as i32, libc::SIGTERM);
                    }
                }
            }
            return;
        }
    };

    let targets: Vec<u32> = if pids_to_kill.is_empty() {
        shm.live_instances()
            .iter()
            .filter_map(|entry| resolve_ui_instance_pid(entry.pid))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    } else {
        pids_to_kill
            .iter()
            .copied()
            .filter_map(resolve_ui_instance_pid)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    };

    if targets.is_empty() {
        println!("No running instances found.");
        return;
    }

    for pid in &targets {
        if sync::is_pid_alive(*pid) {
            unsafe {
                libc::kill(*pid as libc::pid_t, libc::SIGTERM);
            }
            println!("Sent SIGTERM to PID {}", pid);
        } else {
            println!("PID {} is not running", pid);
        }
    }
}

fn cmd_workers() {
    let workers = worker_jobs();
    if workers.is_empty() {
        println!("No worker jobs.");
        return;
    }

    for w in &workers {
        let status = worker_job_status(w);
        let elapsed = chrono::Utc::now().timestamp() - w.started_at;
        let size_str = format_size(w.output_len);
        println!(
            "  PID {:>7}  {:>8}  chat:{}  {:>7}  {}s  {}",
            w.worker_pid, w.provider, &w.chat_id, status, elapsed, size_str,
        );
    }
}

fn cmd_restart_provider(provider: ModelProvider) {
    let prov = provider.as_str();

    let shm = match SyncShm::open_readonly() {
        Some(s) => s,
        None => {
            eprintln!("No shared memory region found. Cannot restart provider {prov}.");
            return;
        }
    };

    let instances = shm.list_instances();
    let targets: Vec<u32> = instances
        .iter()
        .filter(|i| i.provider == provider && sync::is_pid_alive(i.pid))
        .filter_map(|i| resolve_ui_instance_pid(i.pid))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    if targets.is_empty() {
        println!("No running {prov} instances found.");
        return;
    }

    for pid in &targets {
        if sync::is_pid_alive(*pid) {
            unsafe {
                libc::kill(*pid as libc::pid_t, libc::SIGTERM);
            }
            println!("Sent SIGTERM to PID {} ({prov})", pid);
        }
    }

    drop(shm);
    std::thread::sleep(std::time::Duration::from_secs(1));

    println!("Launching replacement {prov} instance...");
    let repo_root = std::env::current_dir().unwrap_or_default();
    let release_dir = configured_release_dir(&repo_root);
    let binary = release_dir.join(chatbot_lib::config::UI_BINARY_NAME);
    let ctl_binary = release_dir.join("chatbot-ctl");
    let adapter_binary = adapter_release_binary_path(&repo_root);

    if !binary.exists() || !adapter_binary.exists() {
        eprintln!(
            "Release artifacts missing (ui: {}, adapter: {}) - building first...",
            binary.display(),
            adapter_binary.display()
        );
        if let Err(err) = build_release_ui_stack(&repo_root) {
            eprintln!("Build failed: {err:#}");
            return;
        }
    }

    if let Err(err) = std::process::Command::new(&ctl_binary)
        .arg("launch")
        .arg("--provider")
        .arg(prov)
        .spawn()
    {
        eprintln!("Failed to restart {prov}: {err}");
    } else {
        println!("Restarted {prov}.");
    }
}

fn cmd_stop_workers(chat_id_filter: Option<&str>) {
    let workers = worker_jobs();
    let mut stopped = 0;

    for w in &workers {
        if w.state != WorkerState::Running {
            continue; // Not running
        }
        if let Some(filter) = chat_id_filter {
            if w.chat_id != filter {
                continue;
            }
        }
        if sync::is_pid_alive(w.worker_pid) {
            unsafe {
                libc::kill(w.worker_pid as libc::pid_t, libc::SIGTERM);
            }
            println!(
                "Sent SIGTERM to worker PID {} (chat:{})",
                w.worker_pid, w.chat_id
            );
            stopped += 1;
        } else {
            println!("Worker PID {} is dead (chat:{})", w.worker_pid, w.chat_id);
        }
    }

    if stopped == 0 {
        println!("No running workers to stop.");
    }
}

fn cmd_gc() {
    let cleaned = cleanup_gc_targets();

    for target in &cleaned {
        match target.worker_pid {
            Some(worker_pid) => println!(
                "Cleaning job (PID {} chat:{} state:{})",
                worker_pid, target.chat_id, target.state_label
            ),
            None => println!(
                "Cleaning job (chat:{} state:{})",
                target.chat_id, target.state_label
            ),
        }
    }

    if cleaned.is_empty() {
        println!("Nothing to clean up.");
    } else {
        println!("Cleaned {} stale worker job(s).", cleaned.len());
    }
}

fn cmd_stop_all() {
    // Stop workers first
    cmd_stop_workers(None);
    // Then stop UI instances
    cmd_stop(&[]);
    cmd_stop_broker();
}

fn cmd_stop_provider(provider: ModelProvider) {
    let prov = provider.as_str();
    let shm = match SyncShm::open_readonly() {
        Some(s) => s,
        None => {
            eprintln!("No shared memory region found. Falling back to signal-based stop...");
            for pid in provider_pids_by_name(provider) {
                unsafe {
                    libc::kill(pid as i32, libc::SIGTERM);
                }
                println!("Sent SIGTERM to PID {} ({prov})", pid);
            }
            return;
        }
    };

    let instances = shm.list_instances();
    let targets: Vec<u32> = instances
        .iter()
        .filter(|i| i.provider == provider && sync::is_pid_alive(i.pid))
        .filter_map(|i| resolve_ui_instance_pid(i.pid))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();

    if targets.is_empty() {
        println!("No running {prov} instances found.");
        return;
    }

    for pid in &targets {
        if sync::is_pid_alive(*pid) {
            unsafe {
                libc::kill(*pid as libc::pid_t, libc::SIGTERM);
            }
            println!("Sent SIGTERM to PID {} ({prov})", pid);
        }
    }
}

#[cfg(target_os = "linux")]
fn provider_pids_by_name(provider: ModelProvider) -> Vec<u32> {
    let output = match std::process::Command::new("pgrep")
        .args(["-f", &format!("rust-chatbot.*{}", provider.as_str())])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output)
        .lines()
        .filter_map(|l| l.parse().ok())
        .collect()
}

#[cfg(not(target_os = "linux"))]
fn provider_pids_by_name(_provider: ModelProvider) -> Vec<u32> {
    Vec::new()
}

fn ensure_broker_best_effort() {
    if broker_health().is_running() {
        return;
    }
    if systemd_unit_path().exists() {
        let _ = systemctl(&["start", SERVICE_NAME]);
    }
}

// ── broker management via systemd ───────────────────────────────────

fn systemctl(args: &[&str]) -> bool {
    match Command::new("systemctl").arg("--user").args(args).status() {
        Ok(s) => s.success(),
        Err(e) => {
            eprintln!("Failed to run systemctl: {}", e);
            false
        }
    }
}

fn systemctl_output(args: &[&str]) {
    let _ = Command::new("systemctl").arg("--user").args(args).status();
}

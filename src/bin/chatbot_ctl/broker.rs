fn cmd_install_broker() {
    let broker_bin = broker_binary_path();
    if !broker_bin.exists() {
        eprintln!("Broker binary not found at: {}", broker_bin.display());
        eprintln!("Run: cargo build --release --bin chatbot-broker");
        process::exit(1);
    }

    let unit_dir = systemd_unit_dir();
    let _ = fs::create_dir_all(&unit_dir);

    let unit_content = format!(
        "[Unit]\n\
         Description=Rust Chatbot Sync Broker\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={}\n\
         Restart=on-failure\n\
         RestartSec=1\n\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        broker_bin.display()
    );

    let unit_path = systemd_unit_path();
    if let Err(e) = fs::write(&unit_path, &unit_content) {
        eprintln!("Failed to write unit file: {}", e);
        process::exit(1);
    }
    println!("Wrote {}", unit_path.display());

    if !systemctl(&["daemon-reload"]) {
        eprintln!("Warning: daemon-reload failed");
    }

    if systemctl(&["enable", SERVICE_NAME]) {
        println!("Enabled {} (will start on login)", SERVICE_NAME);
    }

    if systemctl(&["start", SERVICE_NAME]) {
        println!("Started {}", SERVICE_NAME);
    }

    // Brief pause then show status.
    std::thread::sleep(std::time::Duration::from_millis(500));
    systemctl_output(&["status", SERVICE_NAME, "--no-pager"]);
}

fn cmd_uninstall_broker() {
    systemctl(&["stop", SERVICE_NAME]);
    systemctl(&["disable", SERVICE_NAME]);

    let unit_path = systemd_unit_path();
    if unit_path.exists() {
        if let Err(e) = fs::remove_file(&unit_path) {
            eprintln!("Failed to remove {}: {}", unit_path.display(), e);
        } else {
            println!("Removed {}", unit_path.display());
        }
    }

    systemctl(&["daemon-reload"]);
    println!("Broker service uninstalled.");
}

fn cmd_start_broker() {
    // Check if already running without systemd (direct process).
    let health = broker_health();
    if health.is_running() {
        println!("Broker already running (PID {})", health.pid.unwrap_or(0));
        return;
    }

    if systemctl(&["start", SERVICE_NAME]) {
        println!("Broker started.");
    } else {
        eprintln!("systemctl start failed. Is the service installed?");
        eprintln!("Run: chatbot-ctl install-broker");
    }
}

fn cmd_stop_broker() {
    // Try systemctl first.
    if systemctl(&["stop", SERVICE_NAME]) {
        println!("Broker stopped via systemd.");
        return;
    }

    // Fallback: kill via PID file.
    let pid_file = pid_path();
    if let Ok(pid_str) = fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
                println!("Sent SIGTERM to broker PID {}", pid);
                return;
            }
        }
    }

    println!("Broker not running.");
}

fn cmd_restart_broker() {
    if systemctl(&["restart", SERVICE_NAME]) {
        println!("Broker restarted.");
    } else {
        eprintln!("systemctl restart failed. Trying stop + start...");
        cmd_stop_broker();
        std::thread::sleep(std::time::Duration::from_millis(500));
        cmd_start_broker();
    }
}

fn cmd_broker_status() {
    systemctl_output(&["status", SERVICE_NAME, "--no-pager"]);
}

fn cmd_broker_logs() {
    let _ = Command::new("journalctl")
        .args(["--user", "-u", SERVICE_NAME, "-f", "--no-pager"])
        .status();
}

fn spawn_chatbot_with_args(cwd: &Path, args: &[String]) -> std::io::Result<u32> {
    match ensure_backup_scheduler_preflight() {
        Ok(notes) => {
            for note in notes {
                println!("{note}");
            }
        }
        Err(err) => {
            return Err(std::io::Error::other(format!(
                "failed to ensure backup scheduler: {err:#}"
            )));
        }
    }

    let chatbot_bin = chatbot_binary_path();
    let mut cmd = Command::new(&chatbot_bin);
    cmd.current_dir(cwd)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let child = cmd.spawn()?;
    Ok(child.id())
}

fn default_instance_for_provider(provider: ModelProvider) -> u32 {
    match provider {
        ModelProvider::Claude => 1,
        ModelProvider::MiniMax => 2,
        ModelProvider::Qwen => 5,
        ModelProvider::Codex => 3,
        ModelProvider::Nvidia => 7,
        ModelProvider::Gemini => 4,
        ModelProvider::OpenRouter => 8,
        ModelProvider::OmniForge => 11,
        ModelProvider::GeminiForge => 9,
        ModelProvider::GeminiCliForge => 12,
        ModelProvider::MiniMaxForge => 10,
        ModelProvider::Kimi => 13,
        ModelProvider::OpenCode => 14,
        ModelProvider::Hermes => 15,
        ModelProvider::Forge => 16,
        ModelProvider::Claw => 17,
    }
}

fn cmd_launch(
    provider: ModelProvider,
    instance: Option<u32>,
    cwd: Option<PathBuf>,
    extra_args: Vec<String>,
) {
    ensure_broker_best_effort();

    let launch_cwd =
        cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let instance = Some(instance.unwrap_or_else(|| default_instance_for_provider(provider)));
    let mut args = vec![format!("--{}", provider.as_str())];
    if let Some(instance) = instance {
        args.push("--instance".to_string());
        args.push(instance.to_string());
    }
    args.extend(extra_args);

    match spawn_chatbot_with_args(&launch_cwd, &args) {
        Ok(pid) => println!(
            "Launched {} (PID {}) in {}",
            provider.as_str(),
            pid,
            launch_cwd.display()
        ),
        Err(err) => {
            eprintln!("Failed to launch {}: {}", provider.as_str(), err);
            process::exit(1);
        }
    }
}

#[cfg(test)]
mod broker_tests {
    use super::*;

    #[test]
    fn default_instance_for_omniforge_is_dedicated() {
        assert_eq!(default_instance_for_provider(ModelProvider::OmniForge), 11);
        assert_eq!(default_instance_for_provider(ModelProvider::GeminiForge), 9);
        assert_eq!(default_instance_for_provider(ModelProvider::GeminiCliForge), 12);
        assert_eq!(default_instance_for_provider(ModelProvider::MiniMaxForge), 10);
    }
}

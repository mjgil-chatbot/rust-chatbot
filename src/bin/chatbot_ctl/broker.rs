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

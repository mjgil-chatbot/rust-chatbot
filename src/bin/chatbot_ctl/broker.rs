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

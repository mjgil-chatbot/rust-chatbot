fn desktop_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".local/share/applications")
}

fn chatbot_ctl_binary_path() -> PathBuf {
    std::env::current_exe().unwrap_or_else(|_| PathBuf::from("chatbot-ctl"))
}

fn assets_dir() -> PathBuf {
    // Walk up from the binary location to find the repo assets/ dir.
    // e.g. target/release/chatbot-ctl -> ../../assets
    let self_path = std::env::current_exe()
        .ok()
        .and_then(|path| path.canonicalize().ok().or(Some(path)))
        .unwrap_or_default();
    let self_dir = self_path.parent().unwrap_or(Path::new("."));

    // From target/release/ or target/debug/, go up two levels to repo root.
    if let Some(target_dir) = self_dir.parent() {
        if let Some(repo_root) = target_dir.parent() {
            let assets = repo_root.join("assets");
            if assets.is_dir() {
                return assets;
            }
        }
    }

    // Fallback: try relative to CWD.
    PathBuf::from("assets")
}

fn chatbot_binary_path() -> PathBuf {
    resolve_binary_in_current_profile(&[chatbot_lib::config::UI_BINARY_NAME])
        .unwrap_or_else(exit_on_binary_resolution_error)
}

fn cmd_install_desktop() {
    let dest = desktop_dir();
    let _ = fs::create_dir_all(&dest);

    let ctl_bin = chatbot_ctl_binary_path();
    if !ctl_bin.exists() {
        eprintln!("Binary not found at: {}", ctl_bin.display());
        eprintln!("Run: cargo build --release");
        process::exit(1);
    }

    let entries = [
        (2u32, ModelProvider::MiniMax, "minimax_logo.png"),
        (3u32, ModelProvider::Codex, "openai_logo.png"),
        (13u32, ModelProvider::Kimi, "qwen_logo.png"),
    ];

    for (instance, provider, icon_name) in entries {
        let icon = assets_dir().join(icon_name);
        if !icon.exists() {
            eprintln!("Icon not found at: {}", icon.display());
            process::exit(1);
        }

        let wm_class = chatbot_lib::config::window_app_id(provider, Some(instance));
        let entry = format!(
            "[Desktop Entry]\n\
             Type=Application\n\
             Name=0{instance} - Rust Chatbot ({provider_name})\n\
             Exec={bin} launch --provider {provider} --instance {instance}\n\
             Icon={icon}\n\
             StartupWMClass={wm_class}\n\
             Terminal=false\n\
             Categories=Utility;\n",
            instance = instance,
            bin = ctl_bin.display(),
            provider = provider.as_str(),
            provider_name = provider.display_name(),
            icon = icon.display(),
            wm_class = wm_class,
        );

        let path = dest.join(format!("0{}-rust-chatbot.desktop", instance));
        if let Err(e) = fs::write(&path, &entry) {
            eprintln!("Failed to write {}: {}", path.display(), e);
        } else {
            println!("Wrote {}", path.display());
        }
    }

    println!("Desktop entries installed.");
}

fn cmd_uninstall_desktop() {
    let dest = desktop_dir();
    for i in 1..=17 {
        for file_name in [
            format!("0{}-rust-chatbot.desktop", i),
            format!("0{}-claude-desktop-chat.desktop", i),
        ] {
            let path = dest.join(file_name);
            if path.exists() {
                if let Err(e) = fs::remove_file(&path) {
                    eprintln!("Failed to remove {}: {}", path.display(), e);
                } else {
                    println!("Removed {}", path.display());
                }
            }
        }
    }
    println!("Desktop entries uninstalled.");
}

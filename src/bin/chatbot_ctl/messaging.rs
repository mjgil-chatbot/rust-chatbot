#[cfg(target_os = "linux")]
fn snapshot_live_instances() -> Vec<InstanceSnapshot> {
    let Some(shm) = SyncShm::open_readonly() else {
        return Vec::new();
    };

    let placements = window_placements_by_pid();
    let mut seen_pids = std::collections::BTreeSet::new();
    let mut snapshots = Vec::new();
    for entry in shm.live_instances() {
        let Some(pid) = resolve_ui_instance_pid(entry.pid) else {
            continue;
        };
        if !seen_pids.insert(pid) {
            continue;
        }
        let cwd = match fs::read_link(format!("/proc/{}/cwd", pid)) {
            Ok(path) => path,
            Err(_) => continue,
        };
        let cmdline = match fs::read(format!("/proc/{}/cmdline", pid)) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        let parts: Vec<String> = cmdline
            .split(|b| *b == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect();
        if parts.len() < 2 {
            continue;
        }
        snapshots.push(InstanceSnapshot {
            pid,
            cwd,
            args: parts.into_iter().skip(1).collect(),
            window_placement: placements.get(&pid).copied(),
        });
    }
    snapshots
}

#[cfg(not(target_os = "linux"))]
fn snapshot_live_instances() -> Vec<InstanceSnapshot> {
    let Some(shm) = SyncShm::open_readonly() else {
        return Vec::new();
    };

    let mut seen_pids = std::collections::BTreeSet::new();
    let mut snapshots = Vec::new();
    for entry in shm.live_instances() {
        if !sync::is_pid_alive(entry.pid) || !seen_pids.insert(entry.pid) {
            continue;
        }
        let Some(cwd) = cwd_for_pid(entry.pid) else {
            continue;
        };
        let Some(args) = command_args_for_pid(entry.pid) else {
            continue;
        };
        if args.is_empty() {
            continue;
        }
        snapshots.push(InstanceSnapshot {
            pid: entry.pid,
            cwd,
            args,
            window_placement: None,
        });
    }
    snapshots
}

#[cfg(not(target_os = "linux"))]
fn cwd_for_pid(pid: u32) -> Option<PathBuf> {
    let output = Command::new("lsof")
        .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_lsof_cwd_output(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(target_os = "linux"))]
fn parse_lsof_cwd_output(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

#[cfg(not(target_os = "linux"))]
fn command_args_for_pid(pid: u32) -> Option<Vec<String>> {
    let output = Command::new("ps")
        .args(["-ww", "-p", &pid.to_string(), "-o", "command="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_ps_command_args(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(not(target_os = "linux"))]
fn parse_ps_command_args(output: &str) -> Option<Vec<String>> {
    let parts: Vec<String> = output
        .split_whitespace()
        .map(str::to_string)
        .collect();
    if parts.len() < 2 {
        None
    } else {
        Some(parts.into_iter().skip(1).collect())
    }
}

fn cmd_restart_running() {
    let snapshots = snapshot_live_instances();
    if snapshots.is_empty() {
        println!("No running instances found.");
        return;
    }

    let pids: Vec<u32> = snapshots.iter().map(|snapshot| snapshot.pid).collect();
    cmd_stop(&pids);
    std::thread::sleep(std::time::Duration::from_secs(1));

    for snapshot in snapshots {
        let launch_args = restart_args_for_snapshot(&snapshot);
        match spawn_chatbot_with_args(&snapshot.cwd, &launch_args) {
            Ok(pid) => {
                restore_window_desktop(pid, snapshot.window_placement);
                println!(
                    "Restarted PID {} as {} in {}",
                    snapshot.pid,
                    pid,
                    snapshot.cwd.display()
                );
            }
            Err(err) => eprintln!(
                "Failed to restart PID {} in {}: {}",
                snapshot.pid,
                snapshot.cwd.display(),
                err
            ),
        }
    }
}

#[cfg(target_os = "linux")]
fn window_placements_by_pid() -> std::collections::HashMap<u32, WindowPlacement> {
    let output = match Command::new("wmctrl").args(["-lpG"]).output() {
        Ok(output) if output.status.success() => output,
        _ => return std::collections::HashMap::new(),
    };

    parse_wmctrl_lp_g(&String::from_utf8_lossy(&output.stdout))
}

#[cfg(target_os = "linux")]
fn parse_wmctrl_lp_g(output: &str) -> std::collections::HashMap<u32, WindowPlacement> {
    let mut placements = std::collections::HashMap::new();

    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let _window_id = parts.next();
        let Some(desktop) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };
        let Some(x) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(y) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(width) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };
        let Some(height) = parts.next().and_then(|value| value.parse::<i32>().ok()) else {
            continue;
        };

        placements.entry(pid).or_insert(WindowPlacement {
            desktop,
            x,
            y,
            width,
            height,
        });
    }

    placements
}

#[cfg(target_os = "linux")]
fn restart_args_for_snapshot(snapshot: &InstanceSnapshot) -> Vec<String> {
    let mut args = strip_managed_window_args(&snapshot.args);

    if let Some(placement) = snapshot.window_placement {
        args.extend(placement.startup_args());
    }

    args
}

#[cfg(not(target_os = "linux"))]
fn restart_args_for_snapshot(snapshot: &InstanceSnapshot) -> Vec<String> {
    strip_managed_window_args(&snapshot.args)
}

fn strip_managed_window_args(args: &[String]) -> Vec<String> {
    let mut filtered = Vec::with_capacity(args.len());
    let mut iter = args.iter();

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--window-x" | "--window-y" | "--window-width" | "--window-height" => {
                let _ = iter.next();
            }
            _ => filtered.push(arg.clone()),
        }
    }

    filtered
}

#[cfg(target_os = "linux")]
fn restore_window_desktop(pid: u32, placement: Option<WindowPlacement>) {
    let Some(placement) = placement else {
        return;
    };

    let Some(window_id) = wait_for_window_id_for_pid(pid) else {
        return;
    };

    if placement.desktop == -1 {
        let _ = Command::new("wmctrl")
            .args(["-i", "-r", &window_id, "-b", "add,sticky"])
            .status();
    } else {
        let _ = Command::new("wmctrl")
            .args(["-i", "-r", &window_id, "-b", "remove,sticky"])
            .status();
        let _ = Command::new("wmctrl")
            .args(["-i", "-r", &window_id, "-t", &placement.desktop.to_string()])
            .status();
    }
}

#[cfg(not(target_os = "linux"))]
fn restore_window_desktop(_pid: u32, _placement: Option<WindowPlacement>) {}

#[cfg(target_os = "linux")]
fn wait_for_window_id_for_pid(pid: u32) -> Option<String> {
    for _ in 0..20 {
        if let Some(window_id) = window_id_for_pid(pid) {
            return Some(window_id);
        }

        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    None
}

#[cfg(target_os = "linux")]
fn window_id_for_pid(pid: u32) -> Option<String> {
    let output = Command::new("wmctrl").args(["-lp"]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    parse_wmctrl_lp_for_pid(&String::from_utf8_lossy(&output.stdout), pid)
}

#[cfg(target_os = "linux")]
fn parse_wmctrl_lp_for_pid(output: &str, pid: u32) -> Option<String> {
    for line in output.lines() {
        let mut parts = line.split_whitespace();
        let Some(window_id) = parts.next() else {
            continue;
        };
        let _desktop = parts.next();
        let Some(line_pid) = parts.next().and_then(|value| value.parse::<u32>().ok()) else {
            continue;
        };

        if line_pid == pid {
            return Some(window_id.to_string());
        }
    }

    None
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn parse_wmctrl_lp_g_extracts_window_placements() {
        let output = "\
0x04800007  0  1234  10  20  1000  600  host  Claude\n\
0x0500000a  -1  5678  30  40  1200  700  host  Codex\n";

        let placements = parse_wmctrl_lp_g(output);

        assert_eq!(
            placements.get(&1234),
            Some(&WindowPlacement {
                desktop: 0,
                x: 10,
                y: 20,
                width: 1000,
                height: 600,
            })
        );
        assert_eq!(
            placements.get(&5678),
            Some(&WindowPlacement {
                desktop: -1,
                x: 30,
                y: 40,
                width: 1200,
                height: 700,
            })
        );
    }

    #[test]
    fn strip_managed_window_args_replaces_previous_geometry_flags() {
        let args = vec![
            "--codex".to_string(),
            "--instance".to_string(),
            "3".to_string(),
            "--window-x".to_string(),
            "10".to_string(),
            "--window-y".to_string(),
            "20".to_string(),
            "--window-width".to_string(),
            "1000".to_string(),
            "--window-height".to_string(),
            "600".to_string(),
        ];

        assert_eq!(
            strip_managed_window_args(&args),
            vec![
                "--codex".to_string(),
                "--instance".to_string(),
                "3".to_string()
            ]
        );
    }

    #[test]
    fn parse_wmctrl_lp_for_pid_returns_matching_window_id() {
        let output = "\
0x04800007  0  1234  host  Claude\n\
0x0500000a  1  5678  host  Codex\n";

        assert_eq!(
            parse_wmctrl_lp_for_pid(output, 5678),
            Some("0x0500000a".to_string())
        );
    }

    #[test]
    fn provider_helpers_accept_supported_provider_names_and_flags() {
        for provider in ModelProvider::SUPPORTED {
            assert_eq!(parse_provider_name(provider.as_str()), Some(provider));
            assert_eq!(
                parse_provider_name(&provider.as_str().to_uppercase()),
                Some(provider)
            );
            assert_eq!(
                parse_provider_flag(&format!("--{}", provider.as_str())),
                Some(provider)
            );
            assert_eq!(
                parse_provider_flag(&format!("--provider={}", provider.as_str())),
                Some(provider)
            );
            assert_eq!(
                parse_provider_flag(&format!("--backend={}", provider.as_str())),
                Some(provider)
            );
        }
    }

    #[test]
    fn provider_helpers_reject_disabled_provider_names_and_flags() {
        assert_eq!(parse_provider_name("claude"), None);
        assert_eq!(parse_provider_flag("--claude"), None);
        assert_eq!(parse_provider_flag("--provider=gemini"), None);
    }
}

#[cfg(all(test, not(target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn parse_lsof_cwd_output_extracts_working_directory() {
        let output = "p71102\nfcwd\nn/workspace/rust-chatbot\n";

        assert_eq!(
            parse_lsof_cwd_output(output),
            Some(PathBuf::from("/workspace/rust-chatbot"))
        );
    }

    #[test]
    fn parse_ps_command_args_skips_executable_path() {
        let output = "/workspace/local/bin/rust-chatbot --codex --instance 3\n";

        assert_eq!(
            parse_ps_command_args(output),
            Some(vec![
                "--codex".to_string(),
                "--instance".to_string(),
                "3".to_string()
            ])
        );
    }
}

fn main() {
    chatbot_lib::process_limits::apply_default_process_memory_limit("chatbot-ctl");
    let _debug_guard = rust_debug::init_debug_defaults("chatbot-ctl");
    chatbot_lib::ensure_path_symlink();

    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        print_usage();
        process::exit(1);
    }

    match args[1].as_str() {
        "rebuild-index" => cmd_rebuild_index(),
        "runs" => {
            if args.len() < 3 || args[2] != "list" {
                eprintln!("Usage: chatbot-ctl runs list [--status S] [--provider P] [--limit N] [--workflow W]");
                process::exit(1);
            }
            let mut status: Option<String> = None;
            let mut provider: Option<String> = None;
            let mut limit: Option<usize> = None;
            let mut workflow: Option<String> = None;
            let mut iter = args[3..].iter();
            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--status" => status = iter.next().cloned(),
                    "--provider" => provider = iter.next().cloned(),
                    "--limit" => {
                        limit = iter.next().and_then(|s| s.parse().ok());
                    }
                    "--workflow" | "--mode" => workflow = iter.next().cloned(),
                    _ => {
                        eprintln!("Unknown runs list argument '{}'", arg);
                        process::exit(1);
                    }
                }
            }
            cmd_runs_list(
                status.as_deref(),
                provider.as_deref(),
                limit,
                workflow.as_deref(),
            );
        }
        "stats" => cmd_stats(),
        "prune" => {
            let mut age_days = 30;
            let mut iter = args[2..].iter();
            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--age" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for --age");
                            process::exit(1);
                        };
                        age_days = value.parse().unwrap_or(30);
                    }
                    _ => {
                        eprintln!("Unknown prune argument '{}'", arg);
                        process::exit(1);
                    }
                }
            }
            cmd_prune(age_days);
        }
        "launch" => {
            let mut provider: Option<ModelProvider> = None;
            let mut instance: Option<u32> = None;
            let mut cwd: Option<PathBuf> = None;
            let mut extra_args: Vec<String> = Vec::new();
            let mut iter = args[2..].iter();

            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--provider" | "--backend" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for {}", arg);
                            process::exit(1);
                        };
                        let Some(parsed) = parse_provider_name(value) else {
                            eprintln!("Unknown provider '{}'", value);
                            process::exit(1);
                        };
                        provider = Some(parsed);
                    }
                    "--instance" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for --instance");
                            process::exit(1);
                        };
                        match value.parse::<u32>() {
                            Ok(parsed) => instance = Some(parsed),
                            Err(_) => {
                                eprintln!("Invalid instance '{}'", value);
                                process::exit(1);
                            }
                        }
                    }
                    "--cwd" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for --cwd");
                            process::exit(1);
                        };
                        cwd = Some(PathBuf::from(value));
                    }
                    value => {
                        if let Some(parsed) = parse_provider_flag(value) {
                            provider = Some(parsed);
                        } else if provider.is_none() {
                            if let Some(parsed) = parse_provider_name(value) {
                                provider = Some(parsed);
                            } else {
                                extra_args.push(value.to_string());
                            }
                        } else {
                            extra_args.push(value.to_string());
                        }
                    }
                }
            }

            cmd_launch(
                provider.unwrap_or_else(configured_default_provider),
                instance,
                cwd,
                extra_args,
            );
        }
        "send" => {
            let mut provider: Option<ModelProvider> = None;
            let mut chat_id: Option<String> = None;
            let mut text: Option<String> = None;
            let mut owner_pid_override: Option<u32> = None;
            let mut iter = args[2..].iter();

            while let Some(arg) = iter.next() {
                match arg.as_str() {
                    "--provider" | "--backend" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for {}", arg);
                            process::exit(1);
                        };
                        let Some(parsed) = parse_provider_name(value) else {
                            eprintln!("Unknown provider '{}'", value);
                            process::exit(1);
                        };
                        provider = Some(parsed);
                    }
                    "--chat-id" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for --chat-id");
                            process::exit(1);
                        };
                        chat_id = Some(value.to_string());
                    }
                    "--text" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for --text");
                            process::exit(1);
                        };
                        text = Some(value.to_string());
                    }
                    "--owner-pid" => {
                        let Some(value) = iter.next() else {
                            eprintln!("Missing value for --owner-pid");
                            process::exit(1);
                        };
                        let Ok(parsed) = value.parse::<u32>() else {
                            eprintln!("Invalid value for --owner-pid: '{}'", value);
                            process::exit(1);
                        };
                        owner_pid_override = Some(parsed);
                    }
                    other => {
                        if let Some(parsed) = parse_provider_flag(other) {
                            provider = Some(parsed);
                        } else {
                            eprintln!("Unknown send argument '{}'", other);
                            process::exit(1);
                        }
                    }
                }
            }

            cmd_send(
                provider.unwrap_or_else(configured_default_provider),
                chat_id.as_deref().unwrap_or(""),
                text.as_deref().unwrap_or(""),
                owner_pid_override,
            );
        }
        "run-managed-backup" => {
            let mut force = false;
            for arg in &args[2..] {
                match arg.as_str() {
                    "--force" => force = true,
                    other => {
                        eprintln!("Unknown run-managed-backup argument '{}'", other);
                        process::exit(1);
                    }
                }
            }
            cmd_run_managed_backup(force);
        }
        "ensure-backup-scheduler" => match ensure_backup_scheduler_preflight() {
            Ok(notes) => {
                if notes.is_empty() {
                    println!("Managed backup scheduler already up to date.");
                } else {
                    for note in notes {
                        println!("{note}");
                    }
                }
            }
            Err(err) => {
                eprintln!("Failed to ensure backup scheduler: {err:#}");
                process::exit(1);
            }
        },
        "restart-running" | "rr" => cmd_restart_running(),
        "status" | "st" | "s" => cmd_status(),
        "pids" | "ps" => cmd_pids(),
        "stop" | "kill" | "k" => {
            let pids: Vec<u32> = args[2..].iter().filter_map(|a| a.parse().ok()).collect();
            cmd_stop(&pids);
        }
        "stop-all" => cmd_stop_all(),
        "stop-provider" => {
            let provider = match args.get(2) {
                Some(p) => match parse_provider_name(p) {
                    Some(p) => p,
                    None => {
                        eprintln!("Unknown provider: {}", p);
                        print_usage();
                        process::exit(1);
                    }
                },
                None => {
                    eprintln!("Usage: stop-provider <minimax|codex|kimi>");
                    process::exit(1);
                }
            };
            cmd_stop_provider(provider);
        }
        "workers" | "w" => cmd_workers(),
        "stop-workers" | "sw" => {
            let chat_id = if args.len() > 2 && (args[2] == "--chat-id" || args[2] == "-c") {
                args.get(3).map(|s| s.as_str())
            } else {
                None
            };
            cmd_stop_workers(chat_id);
        }
        "gc" => cmd_gc(),
        "install-desktop" => cmd_install_desktop(),
        "uninstall-desktop" => cmd_uninstall_desktop(),
        "install-broker" => cmd_install_broker(),
        "uninstall-broker" => cmd_uninstall_broker(),
        "start-broker" => cmd_start_broker(),
        "stop-broker" => cmd_stop_broker(),
        "restart-broker" => cmd_restart_broker(),
        "broker-status" => cmd_broker_status(),
        "broker-logs" => cmd_broker_logs(),
        "restart-provider" => {
            let provider = match args.get(2) {
                Some(p) => match parse_provider_name(p) {
                    Some(p) => p,
                    None => {
                        eprintln!("Unknown provider: {}", p);
                        print_usage();
                        process::exit(1);
                    }
                },
                None => {
                    eprintln!("Usage: restart-provider <minimax|codex|kimi>");
                    process::exit(1);
                }
            };
            cmd_restart_provider(provider);
        }
        "help" | "-h" | "--help" => print_usage(),
        other => {
            eprintln!("Unknown command: {}\n", other);
            print_usage();
            process::exit(1);
        }
    }
}

// ── desktop entry management ────────────────────────────────────────

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

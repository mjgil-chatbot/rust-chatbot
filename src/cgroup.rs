//! Cgroup v2 memory limit wrapper for command execution.
//!
//! Provides `wrap_command()` that mutates any `Command` in-place to run inside
//! a systemd-run cgroup scope with memory limits, preventing any spawned
//! process tree from exceeding the configured ceiling.
//!
//! ```no_run
//! use std::process::{Command, Stdio};
//!
//! fn main() -> std::io::Result<()> {
//!     let mut cmd = Command::new("cargo");
//!     cmd.arg("build").arg("--release");
//!     cmd.current_dir("/my/repo");
//!     cmd.stdout(Stdio::piped());
//!     cmd.stderr(Stdio::piped());
//!
//!     chatbot_lib::cgroup::wrap_command(&mut cmd);
//!     let mut child = cmd.spawn()?;
//!     let _ = child.wait()?;
//!     Ok(())
//! }
//! ```
//!
//! Limits applied per cgroup scope:
//!   MemoryMax=9G  (hard limit — OOM kills the whole cgroup if hit)
//!   MemoryHigh=8G (pressure throttle — kernel reclaims before hard limit)
//!   MemorySwapMax=0 (no swap available to the cgroup)
//!   TasksMax=256 (max processes/threads in the cgroup)
//!   OOMPolicy=kill (entire unit is killed together on OOM)

use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Hard memory ceiling for the cgroup. When the cgroup's physical memory
/// usage hits this, the OOM killer targets the entire cgroup unit.
pub const CGROUP_MEMORY_MAX: &str = "9G";

/// Pressure throttle point. The kernel begins reclaiming memory aggressively
/// at this level, before hitting the hard MemoryMax.
pub const CGROUP_MEMORY_HIGH: &str = "8G";

/// Maximum processes/threads allowed inside the cgroup scope.
pub const CGROUP_TASKS_MAX: u64 = 256;

#[cfg(unix)]
pub fn wrap_command(input: &mut Command) {
    let program = input.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = input
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let cwd = input.get_current_dir().map(|p| p.to_path_buf());
    let envs: Vec<_> = input
        .get_envs()
        .map(|(k, v)| {
            (
                k.to_string_lossy().into_owned(),
                v.map(|v| v.to_string_lossy().into_owned()),
            )
        })
        .collect();

    let unit_name = format!(
        "cgroup-wrap-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    let mut cmd = Command::new("systemd-run");
    cmd.arg("--user")
        .arg("--wait")
        .arg("--collect")
        .arg("--pipe")
        .arg(format!("--unit={}", unit_name))
        .arg("-p")
        .arg("Type=exec")
        .arg("-p")
        .arg("MemoryAccounting=yes")
        .arg("-p")
        .arg(format!("MemoryHigh={}", CGROUP_MEMORY_HIGH))
        .arg("-p")
        .arg(format!("MemoryMax={}", CGROUP_MEMORY_MAX))
        .arg("-p")
        .arg("MemorySwapMax=0")
        .arg("-p")
        .arg(format!("TasksMax={}", CGROUP_TASKS_MAX))
        .arg("-p")
        .arg("OOMPolicy=kill")
        .arg("--")
        .arg(&program);

    for arg in &args {
        cmd.arg(arg);
    }

    if let Some(c) = cwd {
        cmd.current_dir(c);
    }
    for (key, val) in envs {
        match val {
            Some(v) => cmd.env(&key, &v),
            None => cmd.env_remove(&key),
        };
    }

    *input = cmd;
}

#[cfg(not(unix))]
pub fn wrap_command(_input: &mut Command) {}

#[cfg(test)]
mod tests {
    #[test]
    fn constants_are_documented() {
        assert_eq!(super::CGROUP_MEMORY_MAX, "9G");
        assert_eq!(super::CGROUP_MEMORY_HIGH, "8G");
        assert_eq!(super::CGROUP_TASKS_MAX, 256);
    }
}

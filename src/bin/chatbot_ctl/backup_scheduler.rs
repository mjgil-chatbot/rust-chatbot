use std::fs::OpenOptions;
use std::io::Write;
use std::process::Stdio;

use chrono::{DateTime, Days, Local, NaiveDate, NaiveDateTime};

const BACKUP_SCHEDULE_HOUR: u32 = 1;
const BACKUP_SCHEDULE_MINUTE: u32 = 15;
const BACKUP_MAX_RETRIES: u32 = 3;
#[cfg(target_os = "linux")]
const BACKUP_SYSTEMD_RESTART_DELAY_SECONDS: u32 = 30;
const BACKUP_SCHEDULER_LOG_FILE: &str = "backup-scheduler.log";
const BACKUP_SUCCESS_STATE_FILE: &str = "backup-last-success.txt";
const BACKUP_RUNNER_COMMAND: &str = "run-managed-backup";
const LEGACY_BACKUP_CRON_BEGIN: &str = "# BEGIN rust-chatbot backups";
const LEGACY_BACKUP_CRON_END: &str = "# END rust-chatbot backups";

#[cfg(target_os = "linux")]
const BACKUP_SYSTEMD_SERVICE_NAME: &str = "rust-chatbot-backup.service";
#[cfg(target_os = "linux")]
const BACKUP_SYSTEMD_TIMER_NAME: &str = "rust-chatbot-backup.timer";

#[cfg(target_os = "macos")]
const BACKUP_LAUNCH_AGENT_LABEL: &str = "com.rustchatbot.backup";

fn backup_scheduler_log_path() -> PathBuf {
    chatbot_lib::debug::log_dir().join(BACKUP_SCHEDULER_LOG_FILE)
}

fn backup_success_state_path() -> PathBuf {
    chatbot_lib::config::shared_infra_dir().join(BACKUP_SUCCESS_STATE_FILE)
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent directory {}", parent.display()))?;
    }
    Ok(())
}

fn scheduled_backup_slot_for_date(date: NaiveDate) -> NaiveDateTime {
    date.and_hms_opt(BACKUP_SCHEDULE_HOUR, BACKUP_SCHEDULE_MINUTE, 0)
        .expect("backup schedule time should be valid")
}

fn most_recent_scheduled_backup_slot(now: NaiveDateTime) -> NaiveDateTime {
    let today_slot = scheduled_backup_slot_for_date(now.date());
    if now >= today_slot {
        today_slot
    } else {
        let yesterday = now
            .date()
            .checked_sub_days(Days::new(1))
            .expect("backup schedule should have a previous day");
        scheduled_backup_slot_for_date(yesterday)
    }
}

fn managed_backup_is_due_at(now: DateTime<Local>, last_success: Option<DateTime<Local>>) -> bool {
    let slot = most_recent_scheduled_backup_slot(now.naive_local());
    last_success
        .map(|value| value.naive_local() < slot)
        .unwrap_or(true)
}

fn read_last_successful_backup_time() -> Result<Option<DateTime<Local>>> {
    let path = backup_success_state_path();
    if !path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("failed to read backup state {}", path.display()))?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let parsed = DateTime::parse_from_rfc3339(trimmed)
        .with_context(|| format!("failed to parse backup state {}", path.display()))?;
    Ok(Some(parsed.with_timezone(&Local)))
}

fn write_last_successful_backup_time(now: DateTime<Local>) -> Result<()> {
    let path = backup_success_state_path();
    ensure_parent_dir(&path)?;
    fs::write(&path, format!("{}\n", now.to_rfc3339()))
        .with_context(|| format!("failed to write backup state {}", path.display()))?;
    Ok(())
}

fn append_scheduler_log_line(line: &str) -> Result<()> {
    let log_path = backup_scheduler_log_path();
    ensure_parent_dir(&log_path)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open scheduler log {}", log_path.display()))?;
    writeln!(file, "{line}")?;
    Ok(())
}

fn skip_backup_scheduler_preflight() -> bool {
    std::env::var("RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT")
        .map(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"))
        .unwrap_or(false)
}

fn run_backup_script() -> Result<()> {
    let script_path = backup_script_path()?;
    let repo_root = backup_repo_root()?;
    let log_path = backup_scheduler_log_path();

    ensure_parent_dir(&log_path)?;
    let mut log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("failed to open scheduler log {}", log_path.display()))?;
    writeln!(
        log_file,
        "=== managed backup start {} ===",
        Local::now().to_rfc3339()
    )?;

    let stdout = log_file
        .try_clone()
        .with_context(|| format!("failed to clone scheduler log {}", log_path.display()))?;
    let stderr = log_file
        .try_clone()
        .with_context(|| format!("failed to clone scheduler log {}", log_path.display()))?;

    let status = Command::new("/bin/bash")
        .arg(&script_path)
        .current_dir(&repo_root)
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .status()
        .with_context(|| format!("failed to run {}", script_path.display()))?;

    writeln!(
        log_file,
        "=== managed backup end {} status={} ===",
        Local::now().to_rfc3339(),
        status
    )?;

    if !status.success() {
        anyhow::bail!("backup script exited with status {status}");
    }

    Ok(())
}

fn retry_backup_operation<F>(max_retries: u32, mut run_once: F) -> Result<u32>
where
    F: FnMut(u32, u32) -> Result<()>,
{
    let total_attempts = max_retries + 1;
    for attempt in 1..=total_attempts {
        match run_once(attempt, total_attempts) {
            Ok(()) => return Ok(attempt),
            Err(_err) if attempt < total_attempts => continue,
            Err(err) => {
                return Err(err.context(format!(
                    "managed backup failed after {total_attempts} attempts"
                )));
            }
        }
    }

    unreachable!("backup retry loop should always return")
}

fn run_backup_script_with_retries() -> Result<u32> {
    retry_backup_operation(
        BACKUP_MAX_RETRIES,
        |attempt, total_attempts| match run_backup_script() {
            Ok(()) => Ok(()),
            Err(err) => {
                let _ = append_scheduler_log_line(&format!(
                    "managed backup attempt {attempt}/{total_attempts} failed: {err:#}"
                ));
                if attempt < total_attempts {
                    eprintln!(
                        "Managed backup attempt {attempt}/{total_attempts} failed: {err:#}. Retrying."
                    );
                }
                Err(err)
            }
        },
    )
}

fn cmd_run_managed_backup(force: bool) {
    let now = Local::now();
    let last_success = match read_last_successful_backup_time() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Failed to read managed backup state: {err:#}");
            process::exit(1);
        }
    };

    if !force && !managed_backup_is_due_at(now, last_success) {
        println!("Managed backup is not due; skipping.");
        return;
    }

    if let Err(err) = append_scheduler_log_line(&format!(
        "managed backup triggered at {} (force={force})",
        now.to_rfc3339()
    )) {
        eprintln!("Failed to append scheduler log: {err:#}");
        process::exit(1);
    }

    let attempt_count = match run_backup_script_with_retries() {
        Ok(attempt_count) => attempt_count,
        Err(err) => {
            let _ = append_scheduler_log_line(&format!("managed backup failed: {err:#}"));
            eprintln!("Managed backup failed: {err:#}");
            process::exit(1);
        }
    };

    if let Err(err) = write_last_successful_backup_time(now) {
        eprintln!("Managed backup completed, but failed to update state: {err:#}");
        process::exit(1);
    }

    let _ = append_scheduler_log_line(&format!(
        "managed backup completed successfully at {} after {attempt_count} attempt(s)",
        now.to_rfc3339()
    ));
    println!("Managed backup completed.");
}

fn write_file_if_changed(path: &Path, contents: &str) -> Result<bool> {
    let existing = fs::read_to_string(path).ok();
    if existing.as_deref() == Some(contents) {
        return Ok(false);
    }

    ensure_parent_dir(path)?;
    fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(true)
}

fn remove_legacy_backup_cron_block(contents: &str) -> String {
    let mut cleaned = String::new();
    let mut skipping = false;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == LEGACY_BACKUP_CRON_BEGIN {
            skipping = true;
            continue;
        }
        if skipping {
            if trimmed == LEGACY_BACKUP_CRON_END {
                skipping = false;
            }
            continue;
        }

        cleaned.push_str(line);
        cleaned.push('\n');
    }

    cleaned
}

#[cfg(target_os = "linux")]
fn systemd_quote_argument(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "linux")]
fn systemd_backup_service_path() -> PathBuf {
    systemd_unit_dir().join(BACKUP_SYSTEMD_SERVICE_NAME)
}

#[cfg(target_os = "linux")]
fn systemd_backup_timer_path() -> PathBuf {
    systemd_unit_dir().join(BACKUP_SYSTEMD_TIMER_NAME)
}

#[cfg(target_os = "linux")]
fn systemd_backup_service_contents(chatbot_ctl: &Path) -> String {
    format!(
        "[Unit]\nDescription=rust-chatbot managed backup\n\n[Service]\nType=oneshot\nExecStart={} {}\nRestart=on-failure\nRestartSec={}s\n",
        systemd_quote_argument(&chatbot_ctl.to_string_lossy()),
        systemd_quote_argument(BACKUP_RUNNER_COMMAND),
        BACKUP_SYSTEMD_RESTART_DELAY_SECONDS,
    )
}

#[cfg(target_os = "linux")]
fn systemd_backup_timer_contents() -> String {
    format!(
        "[Unit]\nDescription=Run rust-chatbot managed backup daily\n\n[Timer]\nOnCalendar=*-*-* {hour:02}:{minute:02}:00\nPersistent=true\nUnit={service}\n\n[Install]\nWantedBy=timers.target\n",
        hour = BACKUP_SCHEDULE_HOUR,
        minute = BACKUP_SCHEDULE_MINUTE,
        service = BACKUP_SYSTEMD_SERVICE_NAME,
    )
}

#[cfg(target_os = "linux")]
fn run_systemctl_user(args: &[&str]) -> Result<()> {
    let status = Command::new("systemctl")
        .arg("--user")
        .args(args)
        .status()
        .with_context(|| format!("failed to run systemctl --user {}", args.join(" ")))?;
    if !status.success() {
        anyhow::bail!(
            "systemctl --user {} exited with status {}",
            args.join(" "),
            status
        );
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_crontab_contents_if_available() -> Result<Option<String>> {
    let output = match Command::new("crontab").arg("-l").output() {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("failed to run `crontab -l`"),
    };

    if output.status.success() {
        return String::from_utf8(output.stdout)
            .context("`crontab -l` returned non-UTF-8 output")
            .map(Some);
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.to_ascii_lowercase().contains("no crontab") {
        return Ok(Some(String::new()));
    }

    anyhow::bail!("`crontab -l` failed: {}", stderr.trim());
}

#[cfg(target_os = "linux")]
fn write_crontab_contents(contents: &str) -> Result<()> {
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to run `crontab -`")?;

    {
        let mut stdin = child
            .stdin
            .take()
            .context("failed to open stdin for `crontab -`")?;
        stdin
            .write_all(contents.as_bytes())
            .context("failed to write updated crontab contents")?;
    }

    let status = child
        .wait()
        .context("failed to wait for `crontab -` to finish")?;
    if !status.success() {
        anyhow::bail!("`crontab -` exited with status {status}");
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn cleanup_legacy_backup_crontab() -> Result<bool> {
    let Some(current) = read_crontab_contents_if_available()? else {
        return Ok(false);
    };

    let updated = remove_legacy_backup_cron_block(&current);
    if updated == current {
        return Ok(false);
    }

    write_crontab_contents(&updated)?;
    Ok(true)
}

#[cfg(target_os = "linux")]
fn ensure_linux_backup_scheduler() -> Result<Vec<String>> {
    let mut notes = Vec::new();
    let chatbot_ctl = current_executable_path()?;
    let service_changed = write_file_if_changed(
        &systemd_backup_service_path(),
        &systemd_backup_service_contents(&chatbot_ctl),
    )?;
    let timer_changed = write_file_if_changed(
        &systemd_backup_timer_path(),
        &systemd_backup_timer_contents(),
    )?;
    let cron_removed = cleanup_legacy_backup_crontab()?;

    if service_changed || timer_changed {
        run_systemctl_user(&["daemon-reload"])?;
        notes.push("Updated rust-chatbot backup systemd units.".to_string());
    }
    if cron_removed {
        notes.push("Removed legacy rust-chatbot backup cron block.".to_string());
    }

    run_systemctl_user(&["enable", BACKUP_SYSTEMD_TIMER_NAME])?;
    run_systemctl_user(&["restart", BACKUP_SYSTEMD_TIMER_NAME])?;

    Ok(notes)
}

#[cfg(target_os = "macos")]
fn launch_agent_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("Library")
        .join("LaunchAgents")
}

#[cfg(target_os = "macos")]
fn launch_agent_path() -> PathBuf {
    launch_agent_dir().join(format!("{BACKUP_LAUNCH_AGENT_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn launch_agent_contents(chatbot_ctl: &Path) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n    <key>Label</key>\n    <string>{label}</string>\n    <key>ProgramArguments</key>\n    <array>\n        <string>{ctl}</string>\n        <string>{command}</string>\n    </array>\n    <key>RunAtLoad</key>\n    <true/>\n    <key>StartCalendarInterval</key>\n    <dict>\n        <key>Hour</key>\n        <integer>{hour}</integer>\n        <key>Minute</key>\n        <integer>{minute}</integer>\n    </dict>\n</dict>\n</plist>\n",
        label = BACKUP_LAUNCH_AGENT_LABEL,
        ctl = xml_escape(&chatbot_ctl.to_string_lossy()),
        command = BACKUP_RUNNER_COMMAND,
        hour = BACKUP_SCHEDULE_HOUR,
        minute = BACKUP_SCHEDULE_MINUTE,
    )
}

#[cfg(target_os = "macos")]
fn launchctl_domain() -> String {
    format!("gui/{}", unsafe { libc::geteuid() })
}

#[cfg(target_os = "macos")]
fn launchctl_service_target() -> String {
    format!("{}/{}", launchctl_domain(), BACKUP_LAUNCH_AGENT_LABEL)
}

#[cfg(target_os = "macos")]
fn launchctl_status(args: &[&str]) -> Result<std::process::ExitStatus> {
    Command::new("launchctl")
        .args(args)
        .status()
        .with_context(|| format!("failed to run launchctl {}", args.join(" ")))
}

#[cfg(target_os = "macos")]
fn launch_agent_loaded() -> Result<bool> {
    Ok(launchctl_status(&["print", &launchctl_service_target()])?.success())
}

#[cfg(target_os = "macos")]
fn ensure_macos_backup_scheduler() -> Result<Vec<String>> {
    let chatbot_ctl = current_executable_path()?;
    let plist_path = launch_agent_path();
    let plist_changed = write_file_if_changed(&plist_path, &launch_agent_contents(&chatbot_ctl))?;
    let was_loaded = launch_agent_loaded()?;
    let mut notes = Vec::new();

    if plist_changed || !was_loaded {
        let domain = launchctl_domain();
        let plist = plist_path.to_string_lossy().into_owned();
        let _ = launchctl_status(&["bootout", &domain, &plist]);
        let bootstrap_status = launchctl_status(&["bootstrap", &domain, &plist])?;
        if !bootstrap_status.success() {
            anyhow::bail!("launchctl bootstrap exited with status {bootstrap_status}");
        }
        notes.push("Updated rust-chatbot backup LaunchAgent.".to_string());
    }

    let enable_status = launchctl_status(&["enable", &launchctl_service_target()])?;
    if !enable_status.success() {
        anyhow::bail!("launchctl enable exited with status {enable_status}");
    }

    Ok(notes)
}

fn ensure_backup_scheduler_preflight() -> Result<Vec<String>> {
    if skip_backup_scheduler_preflight() {
        return Ok(vec![
            "Skipping backup scheduler preflight because RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT is set."
                .to_string(),
        ]);
    }

    #[cfg(target_os = "linux")]
    {
        return ensure_linux_backup_scheduler();
    }

    #[cfg(target_os = "macos")]
    {
        return ensure_macos_backup_scheduler();
    }

    #[allow(unreachable_code)]
    Ok(Vec::new())
}

#[cfg(test)]
mod backup_scheduler_tests {
    use super::*;

    #[test]
    fn most_recent_slot_uses_today_after_schedule_time() {
        let now = NaiveDate::from_ymd_opt(2026, 4, 17)
            .unwrap()
            .and_hms_opt(8, 30, 0)
            .unwrap();

        assert_eq!(
            most_recent_scheduled_backup_slot(now),
            NaiveDate::from_ymd_opt(2026, 4, 17)
                .unwrap()
                .and_hms_opt(1, 15, 0)
                .unwrap()
        );
    }

    #[test]
    fn most_recent_slot_uses_previous_day_before_schedule_time() {
        let now = NaiveDate::from_ymd_opt(2026, 4, 17)
            .unwrap()
            .and_hms_opt(0, 30, 0)
            .unwrap();

        assert_eq!(
            most_recent_scheduled_backup_slot(now),
            NaiveDate::from_ymd_opt(2026, 4, 16)
                .unwrap()
                .and_hms_opt(1, 15, 0)
                .unwrap()
        );
    }

    #[test]
    fn remove_legacy_backup_cron_block_preserves_other_entries() {
        let original = "\
# BEGIN rust-chatbot peak notifications
0 2 * * 1-5 /tmp/notify.sh
# END rust-chatbot peak notifications

# BEGIN rust-chatbot backups
15 1 * * * /tmp/backup.sh
# END rust-chatbot backups
";

        let cleaned = remove_legacy_backup_cron_block(original);

        assert!(cleaned.contains("/tmp/notify.sh"));
        assert!(!cleaned.contains("/tmp/backup.sh"));
        assert!(!cleaned.contains(LEGACY_BACKUP_CRON_BEGIN));
        assert!(!cleaned.contains(LEGACY_BACKUP_CRON_END));
    }

    #[test]
    fn retry_backup_operation_retries_until_success() {
        let mut attempts = 0;

        let successful_attempt =
            retry_backup_operation(BACKUP_MAX_RETRIES, |attempt, total_attempts| {
                attempts = attempt;
                assert_eq!(total_attempts, BACKUP_MAX_RETRIES + 1);
                if attempt < 3 {
                    anyhow::bail!("attempt {attempt} failed");
                }
                Ok(())
            })
            .expect("backup should succeed before retries are exhausted");

        assert_eq!(successful_attempt, 3);
        assert_eq!(attempts, 3);
    }

    #[test]
    fn retry_backup_operation_fails_after_exhausting_retries() {
        let mut attempts = 0;

        let err = retry_backup_operation(BACKUP_MAX_RETRIES, |attempt, _| {
            attempts = attempt;
            anyhow::bail!("attempt {attempt} failed");
        })
        .expect_err("backup should fail after exhausting retries");

        assert_eq!(attempts, BACKUP_MAX_RETRIES + 1);
        let error_text = format!("{err:#}");
        assert!(error_text.contains("managed backup failed after 4 attempts"));
        assert!(error_text.contains("attempt 4 failed"));
    }

    #[test]
    fn skip_backup_scheduler_preflight_defaults_to_false() {
        unsafe {
            std::env::remove_var("RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT");
        }

        assert!(!skip_backup_scheduler_preflight());
    }

    #[test]
    fn skip_backup_scheduler_preflight_accepts_truthy_values() {
        unsafe {
            std::env::set_var("RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT", "true");
        }

        assert!(skip_backup_scheduler_preflight());

        unsafe {
            std::env::set_var("RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT", "ON");
        }

        assert!(skip_backup_scheduler_preflight());

        unsafe {
            std::env::remove_var("RUST_CHATBOT_SKIP_BACKUP_SCHEDULER_PREFLIGHT");
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_timer_contents_enable_persistent_catchup() {
        let timer = systemd_backup_timer_contents();

        assert!(timer.contains("Persistent=true"));
        assert!(timer.contains("OnCalendar=*-*-* 01:15:00"));
        assert!(timer.contains(BACKUP_SYSTEMD_SERVICE_NAME));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn systemd_service_contents_restart_after_failure() {
        let service = systemd_backup_service_contents(Path::new("/Applications/chatbot-ctl"));

        assert!(service.contains("Type=oneshot"));
        assert!(service.contains("Restart=on-failure"));
        assert!(service.contains(&format!(
            "RestartSec={}s",
            BACKUP_SYSTEMD_RESTART_DELAY_SECONDS
        )));
        assert!(service.contains(BACKUP_RUNNER_COMMAND));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent_contents_enable_run_at_load_and_daily_schedule() {
        let plist = launch_agent_contents(Path::new("/Applications/chatbot-ctl"));

        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(plist.contains("<true/>"));
        assert!(plist.contains("<key>StartCalendarInterval</key>"));
        assert!(plist.contains("<integer>1</integer>"));
        assert!(plist.contains("<integer>15</integer>"));
    }
}

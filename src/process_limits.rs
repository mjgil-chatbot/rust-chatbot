use std::env;
pub const DEFAULT_PROCESS_MEMORY_LIMIT_BYTES: u64 = 9 * 1024 * 1024 * 1024;
pub const PROCESS_MEMORY_LIMIT_BYTES_ENV: &str = "RUST_CHATBOT_PROCESS_MEMORY_LIMIT_BYTES";
pub fn apply_default_process_memory_limit(process_name: &str) {
    let Some(limit_bytes) = configured_process_memory_limit_bytes() else {
        return;
    };
    if let Err(err) = apply_process_memory_limit_bytes(limit_bytes) {
        eprintln!(
            "warning: failed to apply process memory limit for {} ({} bytes): {}",
            process_name, limit_bytes, err
        );
    }
}
fn configured_process_memory_limit_bytes() -> Option<u64> {
    let Some(raw) = env::var_os(PROCESS_MEMORY_LIMIT_BYTES_ENV) else {
        return Some(DEFAULT_PROCESS_MEMORY_LIMIT_BYTES);
    };
    let Ok(raw) = raw.into_string() else {
        eprintln!(
            "warning: ignoring non-utf8 {} value; using default {} bytes",
            PROCESS_MEMORY_LIMIT_BYTES_ENV, DEFAULT_PROCESS_MEMORY_LIMIT_BYTES
        );
        return Some(DEFAULT_PROCESS_MEMORY_LIMIT_BYTES);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Some(DEFAULT_PROCESS_MEMORY_LIMIT_BYTES);
    }
    if trimmed == "0" {
        return None;
    }
    match trimmed.parse::<u64>() {
        Ok(limit) if limit > 0 => Some(limit),
        _ => {
            eprintln!(
                "warning: ignoring invalid {} value `{}`; using default {} bytes",
                PROCESS_MEMORY_LIMIT_BYTES_ENV, trimmed, DEFAULT_PROCESS_MEMORY_LIMIT_BYTES
            );
            Some(DEFAULT_PROCESS_MEMORY_LIMIT_BYTES)
        }
    }
}
#[cfg(unix)]
fn apply_process_memory_limit_bytes(limit_bytes: u64) -> std::io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: limit_bytes as libc::rlim_t,
        rlim_max: limit_bytes as libc::rlim_t,
    };
    let rc = unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}
#[cfg(not(unix))]
fn apply_process_memory_limit_bytes(_limit_bytes: u64) -> std::io::Result<()> {
    Ok(())
}

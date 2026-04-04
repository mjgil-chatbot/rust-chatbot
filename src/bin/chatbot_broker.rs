//! chatbot-broker — dedicated relay daemon for cross-instance sync.
//!
//! Pure message relay. Reads length-prefixed frames from any client,
//! forwards to all OTHER connected clients. Treats all message content
//! as opaque bytes — has zero knowledge of chats, providers, or sessions.

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

static RUNNING: AtomicBool = AtomicBool::new(true);

fn main() {
    chatbot_lib::process_limits::apply_default_process_memory_limit("chatbot-broker");
    let _debug_guard = rust_debug::init_debug_defaults("chatbot-broker");
    rust_debug::install_panic_hook();
    chatbot_lib::ensure_path_symlink();

    let data_dir = chatbot_lib::config::shared_infra_dir();
    let _ = fs::create_dir_all(&data_dir);

    let sock_path = chatbot_lib::sync::broker_socket_path();
    let pid_path = chatbot_lib::sync::broker_pid_path();

    // Idempotent: exit if another broker is already running.
    if is_broker_running(&sock_path, &pid_path) {
        eprintln!("broker: already running, exiting");
        std::process::exit(0);
    }

    // Remove stale socket file.
    if sock_path.exists() {
        let _ = fs::remove_file(&sock_path);
    }

    let listener = match UnixListener::bind(&sock_path) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("broker: failed to bind {}: {}", sock_path.display(), e);
            std::process::exit(1);
        }
    };
    if let Err(e) = listener.set_nonblocking(true) {
        eprintln!("broker: failed to set non-blocking mode: {}", e);
        std::process::exit(1);
    }

    let _ = fs::write(&pid_path, format!("{}", std::process::id()));

    eprintln!(
        "broker: listening on {} (PID {})",
        sock_path.display(),
        std::process::id()
    );

    // SIGTERM / SIGINT → set RUNNING to false (via sigaction for portability).
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = signal_handler as *const () as libc::sighandler_t;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;
        libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut());
        libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut());
    }

    let clients: Arc<Mutex<HashMap<u64, UnixStream>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut next_id: u64 = 0;

    while RUNNING.load(Ordering::Acquire) {
        match listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(false).ok();
                let id = next_id;
                next_id += 1;

                let write_clone = match stream.try_clone() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                clients
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(id, write_clone);

                let clients_ref = Arc::clone(&clients);
                thread::spawn(move || relay_reader(id, stream, clients_ref));
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("broker: accept error: {}", e);
                break;
            }
        }
    }

    eprintln!("broker: shutting down");
    let _ = fs::remove_file(&sock_path);
    let _ = fs::remove_file(&pid_path);
}

extern "C" fn signal_handler(_: libc::c_int) {
    RUNNING.store(false, Ordering::Release);
}

/// Read frames from one client and relay to all others.
fn relay_reader(id: u64, mut stream: UnixStream, clients: Arc<Mutex<HashMap<u64, UnixStream>>>) {
    loop {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).is_err() {
            clients
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            return;
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        if len > 16 * 1024 * 1024 {
            clients
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            return;
        }

        let mut payload = vec![0u8; len];
        if stream.read_exact(&mut payload).is_err() {
            clients
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&id);
            return;
        }

        let mut frame = Vec::with_capacity(4 + len);
        frame.extend_from_slice(&len_buf);
        frame.extend_from_slice(&payload);

        // Snapshot recipients while holding the lock, then release before writing.
        let peers: Vec<(u64, UnixStream)> = {
            let guard = clients.lock().unwrap_or_else(|e| e.into_inner());
            guard
                .iter()
                .filter(|(&cid, _)| cid != id)
                .filter_map(|(&cid, s)| s.try_clone().ok().map(|s| (cid, s)))
                .collect()
        };

        let mut dead = Vec::new();
        for (cid, mut writer) in peers {
            if writer.write_all(&frame).is_err() {
                dead.push(cid);
            }
        }
        if !dead.is_empty() {
            let mut guard = clients.lock().unwrap_or_else(|e| e.into_inner());
            for cid in dead {
                guard.remove(&cid);
            }
        }
    }
}

fn is_broker_running(sock_path: &std::path::Path, pid_path: &std::path::Path) -> bool {
    // Check PID file first.
    if let Ok(pid_str) = fs::read_to_string(pid_path) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            if unsafe { libc::kill(pid, 0) } == 0 {
                // Process exists. Verify it's actually listening.
                if UnixStream::connect(sock_path).is_ok() {
                    return true;
                }
            }
        }
    }
    // Also try connecting directly (maybe PID file is stale but socket works).
    if sock_path.exists() && UnixStream::connect(sock_path).is_ok() {
        return true;
    }
    false
}

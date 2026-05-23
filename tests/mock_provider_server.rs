// Reusable mock provider server fixture for deterministic parity testing.
//
// Provides a fake Anthropic-compatible HTTP backend, a synthetic Claude CLI,
// and worker harness helpers so that integration tests can drive the real
// worker binary against predetermined responses and assert request ordering,
// request counts, config injection, and session propagation at the provider
// boundary.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chatbot_lib::worker::{self, WorkerJobSnapshot, WorkerRequest, WorkerResult};
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// Environment variable names mirroring src/provider_bridge.rs test overrides
// ---------------------------------------------------------------------------

pub const TEST_PROVIDER_CLI_ENV: &str = "RUST_CHATBOT_TEST_PROVIDER_CLI";
pub const TEST_PROVIDER_CONFIG_DIR_ENV: &str = "RUST_CHATBOT_TEST_PROVIDER_CONFIG_DIR";
pub const TEST_ANTHROPIC_BASE_URL_ENV: &str = "RUST_CHATBOT_TEST_ANTHROPIC_BASE_URL";
pub const TEST_ANTHROPIC_AUTH_TOKEN_ENV: &str = "RUST_CHATBOT_TEST_ANTHROPIC_AUTH_TOKEN";
pub const TEST_MCP_JSON_PATH_ENV: &str = "RUST_CHATBOT_TEST_MCP_JSON_PATH";
pub const TEST_CLAUDE_JSON_PATH_ENV: &str = "RUST_CHATBOT_TEST_CLAUDE_JSON_PATH";
pub const TEST_GEMINI_SETTINGS_PATH_ENV: &str = "RUST_CHATBOT_TEST_GEMINI_SETTINGS_PATH";
pub const TEST_CODEX_TOML_PATH_ENV: &str = "RUST_CHATBOT_TEST_CODEX_TOML_PATH";
pub const TEST_AGENT_ADAPTER_CLI_ENV: &str = "RUST_CHATBOT_AGENT_ADAPTER_CLI";

// ---------------------------------------------------------------------------
// TempDir: auto-cleaned directory for test isolation
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(label: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "rust-chatbot-parity-{}-{}-{}",
            label,
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// ScopedEnv: RAII-style environment variable overrides
// ---------------------------------------------------------------------------

pub struct ScopedEnv {
    saved: Vec<(String, Option<OsString>)>,
}

impl ScopedEnv {
    pub fn set<I, K, V>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: AsRef<std::ffi::OsStr>,
    {
        let mut saved = Vec::new();
        for (key, value) in pairs {
            let key = key.into();
            saved.push((key.clone(), std::env::var_os(&key)));
            std::env::set_var(&key, value);
        }
        Self { saved }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        while let Some((key, value)) = self.saved.pop() {
            match value {
                Some(value) => std::env::set_var(&key, value),
                None => std::env::remove_var(&key),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ScenarioStep + RecordedRequest: scenario definition and request logging
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct ScenarioStep {
    pub name: &'static str,
    pub response: Value,
}

#[derive(Clone, Debug)]
pub struct RecordedRequest {
    pub step: String,
    pub path: String,
    pub auth_token: Option<String>,
    pub body: Value,
}

impl RecordedRequest {
    /// Returns the model sent in the request body.
    pub fn model(&self) -> Option<&str> {
        self.body.get("model").and_then(Value::as_str)
    }

    /// Returns the system prompt sent in the request body.
    pub fn system_prompt(&self) -> Option<&str> {
        self.body.get("system").and_then(Value::as_str)
    }

    /// Returns the resume_session_id sent in the request body.
    pub fn resume_session_id(&self) -> Option<&str> {
        self.body.get("resume_session_id").and_then(Value::as_str)
    }

    /// Returns the user query text from the first user message.
    pub fn user_query(&self) -> Option<&str> {
        self.body
            .get("messages")
            .and_then(|m| m.get(0))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("text"))
            .and_then(Value::as_str)
    }

    /// Returns the tool_result_count from metadata.
    pub fn tool_result_count(&self) -> Option<u64> {
        self.body
            .get("metadata")
            .and_then(|m| m.get("tool_result_count"))
            .and_then(Value::as_u64)
    }
}

// ---------------------------------------------------------------------------
// MockProviderServer: deterministic Anthropic-compatible HTTP backend
// ---------------------------------------------------------------------------

pub struct MockProviderServer {
    addr: SocketAddr,
    request_log: Arc<Mutex<Vec<RecordedRequest>>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MockProviderServer {
    /// Start a mock server that serves the given steps in order, recording
    /// each incoming request for later assertion.
    pub fn start(steps: Vec<ScenarioStep>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let addr = listener.local_addr().unwrap();
        let request_log = Arc::new(Mutex::new(Vec::new()));
        let request_log_for_thread = Arc::clone(&request_log);
        let stop = Arc::new(AtomicBool::new(false));
        let stop_for_thread = Arc::clone(&stop);
        let thread = thread::spawn(move || {
            let mut served = 0usize;
            while served < steps.len() && !stop_for_thread.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        // Accepted sockets inherit O_NONBLOCK on macOS.
                        stream.set_nonblocking(false).unwrap();
                        let request = read_http_request(&mut stream);
                        let step = &steps[served];
                        request_log_for_thread
                            .lock()
                            .unwrap()
                            .push(RecordedRequest {
                                step: step.name.to_string(),
                                path: request.path,
                                auth_token: request.headers.get("x-api-key").cloned(),
                                body: serde_json::from_slice(&request.body).unwrap(),
                            });
                        let response_body = serde_json::to_vec(&step.response).unwrap();
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            response_body.len()
                        )
                        .unwrap();
                        stream.write_all(&response_body).unwrap();
                        stream.flush().unwrap();
                        served += 1;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(err) => panic!("mock provider accept failed: {err}"),
                }
            }
        });

        Self {
            addr,
            request_log,
            stop,
            thread: Some(thread),
        }
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/anthropic", self.addr)
    }

    /// Returns the recorded provider request log in arrival order.
    pub fn request_log(&self) -> Vec<RecordedRequest> {
        self.request_log.lock().unwrap().clone()
    }

    /// Returns a snapshot of all recorded requests in arrival order.
    pub fn requests(&self) -> Vec<RecordedRequest> {
        self.request_log()
    }

    /// Returns the number of requests received so far.
    pub fn request_count(&self) -> usize {
        self.request_log.lock().unwrap().len()
    }

    /// Returns the ordered step names of recorded requests.
    pub fn step_names(&self) -> Vec<String> {
        self.request_log
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.step.clone())
            .collect()
    }
}

impl Drop for MockProviderServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP request parser (minimal, for test use)
// ---------------------------------------------------------------------------

struct HttpRequest {
    path: String,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn read_http_request(stream: &mut TcpStream) -> HttpRequest {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request_line = String::new();
    reader.read_line(&mut request_line).unwrap();
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();

    let mut headers = BTreeMap::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line == "\r\n" || line.is_empty() {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            let normalized = name.trim().to_ascii_lowercase();
            let value = value.trim().to_string();
            if normalized == "content-length" {
                content_length = value.parse().unwrap();
            }
            headers.insert(normalized, value);
        }
    }

    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).unwrap();
    HttpRequest {
        path,
        headers,
        body,
    }
}

// ---------------------------------------------------------------------------
// WorkerHarness: sets up a fully isolated test worker environment
// ---------------------------------------------------------------------------

pub struct WorkerHarness {
    pub _temp: TempDir,
    pub _env: ScopedEnv,
    pub fake_cli_path: PathBuf,
    pub config_dir: PathBuf,
}

impl WorkerHarness {
    pub fn new(label: &str, server: &MockProviderServer) -> Self {
        let temp = TempDir::new(label);
        let home = temp.path().join("home");
        let config_dir = home.join(".rust-chatbot").join(".minimax");
        let missing_mcp = temp.path().join("missing.mcp.json");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&config_dir).unwrap();

        let fake_cli_path = temp.path().join("fake_claude.py");
        write_fake_claude_cli(&fake_cli_path);
        let fake_adapter_path = temp.path().join("fake_agent_adapter_cli.sh");
        write_fake_agent_adapter_cli(&fake_adapter_path);

        let env = ScopedEnv::set([
            ("HOME", home.into_os_string()),
            (
                TEST_PROVIDER_CLI_ENV,
                fake_cli_path.clone().into_os_string(),
            ),
            (
                TEST_PROVIDER_CONFIG_DIR_ENV,
                config_dir.clone().into_os_string(),
            ),
            (
                TEST_ANTHROPIC_BASE_URL_ENV,
                OsString::from(server.base_url()),
            ),
            (
                TEST_ANTHROPIC_AUTH_TOKEN_ENV,
                OsString::from("parity-token"),
            ),
            ("MINIMAX_API_KEY", OsString::from("parity-token")),
            ("ANTHROPIC_BASE_URL", OsString::from(server.base_url())),
            ("ANTHROPIC_AUTH_TOKEN", OsString::from("parity-token")),
            (
                "AGENT_ADAPTER_MINIMAX_EXE",
                fake_cli_path.clone().into_os_string(),
            ),
            (
                "RUST_CHATBOT_MINIMAX_CHATBOT_ADAPTER",
                fake_cli_path.clone().into_os_string(),
            ),
            (
                TEST_AGENT_ADAPTER_CLI_ENV,
                fake_adapter_path.into_os_string(),
            ),
            (TEST_MCP_JSON_PATH_ENV, missing_mcp.into_os_string()),
        ]);

        Self {
            _temp: temp,
            _env: env,
            fake_cli_path,
            config_dir,
        }
    }
}

// ---------------------------------------------------------------------------
// Worker binary helpers
// ---------------------------------------------------------------------------

pub fn worker_binary() -> &'static str {
    env!("CARGO_BIN_EXE_chatbot-worker")
}

pub fn unique_chat_id(name: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("parity-{name}-{}-{nanos}", std::process::id())
}

pub fn make_request(chat_id: &str, query: &str, resume_session_id: Option<&str>) -> WorkerRequest {
    WorkerRequest {
        provider: "minimax".to_string(),
        query: query.to_string(),
        working_dir: Some(chatbot_lib::config::repo_root().to_string()),
        system_prompt: Some("Return deterministic parity output.".to_string()),
        resume_session_id: resume_session_id.map(str::to_string),
        memory_context: None,
        parent_pid_override: Some(std::process::id()),
        runsc_dirs: Vec::new(),
        runsc_image: None,
        model_override: None,
        command: "parity-harness".to_string(),
        chat_id: chat_id.to_string(),
    }
}

pub fn spawn_worker(request: &WorkerRequest) -> Child {
    fs::create_dir_all(worker::worker_dir(&request.chat_id)).unwrap();
    worker::write_json_atomic(&worker::request_path(&request.chat_id), request).unwrap();
    Command::new(worker_binary())
        .arg(&request.chat_id)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

pub fn wait_for_worker(mut child: Child) -> Output {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(_) = child.try_wait().unwrap() {
            return child.wait_with_output().unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "worker did not exit before timeout"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

pub fn wait_until<F>(timeout: Duration, mut predicate: F)
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if predicate() {
            return;
        }
        thread::sleep(Duration::from_millis(20));
    }
    panic!("condition was not satisfied before timeout");
}

pub fn read_worker_result(chat_id: &str) -> WorkerResult {
    serde_json::from_str(&fs::read_to_string(worker::result_path(chat_id)).unwrap()).unwrap()
}

pub fn read_worker_state(chat_id: &str) -> WorkerJobSnapshot {
    worker::read_job_state(chat_id).unwrap().unwrap()
}

#[path = "support/mock_provider_server_helpers.rs"]
mod helpers;
#[path = "support/mock_provider_server_scripts.rs"]
mod scripts;

pub use helpers::*;
pub use scripts::*;

#[cfg(test)]
mod transport_tests {
    use super::*;

    #[test]
    fn accepted_connection_waits_for_delayed_request_bytes() {
        let server = MockProviderServer::start(vec![ScenarioStep {
            name: "delayed-request",
            response: json!({"ok": true}),
        }]);
        let mut client = TcpStream::connect(server.addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        // On macOS an accepted socket inherits the nonblocking listener flag.
        // Let accept finish before sending any bytes, rather than racing it.
        thread::sleep(Duration::from_millis(200));
        client.write_all(b"POST /anthropic/v1/messages HTTP/1.1\r\nHost: localhost\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert_eq!(server.request_count(), 1);
        assert_eq!(server.request_log()[0].body, json!({}));
    }
}

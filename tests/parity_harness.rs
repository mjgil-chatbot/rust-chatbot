// Deterministic parity harness: six canonical scenarios that exercise the
// provider boundary through the real worker binary against a mock provider
// server. Each test asserts request ordering, request counts, auth token
// propagation, and scenario-specific provider-boundary invariants.

#[path = "mock_provider_server.rs"]
mod mock_provider_server;
use std::fs;
use std::sync::Mutex;
use std::time::Duration;
use mock_provider_server::*;
use serde_json::{json, Value};
static ENV_LOCK: Mutex<()> = Mutex::new(());

// Deterministic parity harness: six canonical scenarios that exercise the
// provider boundary through the real worker binary against a mock provider
// server. Each test asserts request ordering, request counts, auth token
// propagation, and scenario-specific provider-boundary invariants.

#[path = "mock_provider_server.rs"]
mod mock_provider_server;

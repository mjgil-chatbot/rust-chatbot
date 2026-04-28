use serde::Deserialize;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct InspectSessionInput {
    /// Rust Chatbot session id to inspect. Accepts either the full id or a unique prefix.
    pub session_id: String,
}

//! chatbot-broker — dedicated relay daemon for cross-instance sync.
//!
//! Pure message relay. Reads length-prefixed frames from any client,
//! forwards to all OTHER connected clients. Treats all message content
//! as opaque bytes — has zero knowledge of chats, providers, or sessions.

fn main() {}

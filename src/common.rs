use std::borrow::Cow;
use crate::data::ChatSession;
use crate::model::ModelProvider;
pub fn worker_request_model_override(
    provider: ModelProvider,
    session: Option<&ChatSession>,
) -> Option<String> {
    if provider != ModelProvider::OmniForge {
        return None;
    }

    Some(crate::omniforge::selected_model(session).model.to_string())
}

//! UiActions adapter for AppState.
//!
//! Bridges AppState actions to chatbot_ui::traits::UiActions.

use std::sync::Arc;
use app::AppState;
use chatbot_lib::data::SessionInfo;
use chatbot_ui::traits::SessionListItem;

pub struct AppStateUiActions<'a> {
    pub state: &'a mut AppState,
}

impl<'a> chatbot_ui::traits::UiActions for AppStateUiActions<'a> {
    fn submit_message(&mut self, text: String) {
        self.state.layout.input_text = text;
        self.state.enqueue_event(app::state::AppEvent::UiAction(
            app::state::AppUiAction::SubmitMessage,
        ));
    }

    fn stop_processing(&mut self) {
        self.state.enqueue_event(app::state::AppEvent::UiAction(
            app::state::AppUiAction::StopCurrentProcess,
        ));
    }

    fn load_session(&mut self, id: &str) {
        self.state.enqueue_event(app::state::AppEvent::UiAction(
            app::state::AppUiAction::LoadSession(id.to_string()),
        ));
    }

    fn create_session(&mut self, name: String, system_prompt: Option<String>) {
        self.state.enqueue_event(app::state::AppEvent::UiAction(
            app::state::AppUiAction::CreateSession { name, system_prompt },
        ));
    }

    fn delete_session(&mut self, id: &str) {
        self.state.enqueue_event(app::state::AppEvent::UiAction(
            app::state::AppUiAction::DeleteSession(id.to_string()),
        ));
    }

    fn rename_session(&mut self, id: &str, name: String) {
        self.state.enqueue_event(app::state::AppEvent::UiAction(
            app::state::AppUiAction::RenameSession {
                session_id: id.to_string(),
                new_name: name,
            },
        ));
    }

    fn select_workspace(&mut self, dir: &str) {
        self.state.select_workspace_dir(dir.to_string());
    }

    fn select_model(&mut self, model_id: &str) {
        let session_id = self.state.session.current_session.as_ref().map(|s| s.id.clone());
        if let Some(session_id) = session_id {
            self.state.enqueue_event(app::state::AppEvent::UiAction(
                app::state::AppUiAction::SaveCurrentOmniForgeModel {
                    session_id,
                    model_id: model_id.to_string(),
                },
            ));
        }
    }

    fn set_system_prompt(&mut self, prompt: Option<String>) {
        let session_id = self.state.session.current_session.as_ref().map(|s| s.id.clone());
        if let Some(session_id) = session_id {
            self.state.enqueue_event(app::state::AppEvent::UiAction(
                app::state::AppUiAction::SaveCurrentSystemPrompt {
                    session_id,
                    system_prompt: prompt,
                },
            ));
        }
    }

    fn toggle_system_prompt(&mut self) {
        self.state.session.show_system_prompt = !self.state.session.show_system_prompt;
    }

    fn show_new_session_dialog(&mut self) {
        self.state.session.show_new_session_dialog = true;
        self.state.session.new_session_name = format!(
            "Chat {}",
            self.state.session.sessions.len() + 1
        );
    }

    fn show_rename_dialog(&mut self, session_id: String) {
        self.state.session.show_rename_session_dialog = true;
        self.state.session.rename_session_id = Some(session_id);
    }

    fn hide_dialogs(&mut self) {
        self.state.session.show_new_session_dialog = false;
        self.state.session.show_rename_session_dialog = false;
    }

    fn get_sessions(&self) -> Vec<Arc<dyn SessionListItem>> {
        self.state
            .session
            .sessions
            .iter()
            .map(|s| Arc::new(s.clone()) as Arc<dyn SessionListItem>)
            .collect()
    }
}
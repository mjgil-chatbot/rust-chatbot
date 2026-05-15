use super::height_seed::should_seed_full_height_cache;
use super::{
    has_complete_height_cache_entry, render_empty_message_area, render_message_area,
    resolve_scroll_adjustment, should_schedule_height_cache_save, should_show_pending_draft,
    SyncHeightCacheSaveGuard,
};
use crate::app::state::{AppState, PendingDraft};
use crate::data::{
    height_cache_width_key, resolve_height_cache_entry, ChatMessage, ChatSession, DataManager,
    HeightCacheEntry, SessionHeightCache, SESSION_HEIGHT_CACHE_VERSION,
};
use crate::model::ModelProvider;
use chrono::{TimeZone, Utc};
use eframe::egui;

mod render_regressions;

#[test]
fn inter_message_spacing_uses_two_body_lines() {
    let ctx = egui::Context::default();
    let mut raw_input = egui::RawInput::default();
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(320.0, 240.0),
    ));

    let mut measured_spacing = None;
    let _ = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let expected = ui.text_style_height(&egui::TextStyle::Body) * 2.0;
            measured_spacing = Some(super::inter_message_spacing(ui));
            assert_eq!(measured_spacing, Some(expected));
        });
    });
}

struct TestHomeGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    _home_dir: tempfile::TempDir,
    previous_home: Option<std::ffi::OsString>,
}

impl TestHomeGuard {
    fn new() -> Self {
        let lock = crate::TEST_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let home_dir = tempfile::TempDir::new().unwrap();
        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", home_dir.path());
        Self {
            _lock: lock,
            _home_dir: home_dir,
            previous_home,
        }
    }
}

impl Drop for TestHomeGuard {
    fn drop(&mut self) {
        match self.previous_home.as_ref() {
            Some(home) => std::env::set_var("HOME", home),
            None => std::env::remove_var("HOME"),
        }
    }
}

fn make_session(id: &str, message_count: usize) -> ChatSession {
    let created_at = Utc.with_ymd_and_hms(2026, 4, 17, 7, 0, 0).unwrap();
    let messages = (0..message_count)
        .map(|index| ChatMessage {
            content: format!("message {}", index + 1),
            is_user: index % 2 == 0,
            timestamp: created_at + chrono::Duration::minutes(index as i64),
            command: None,
            retrieved_memory: None,
        })
        .collect();

    ChatSession {
        id: id.to_string(),
        name: "Height Cache".to_string(),
        system_prompt: None,
        json_summary: None,
        claude_session_id: None,
        minimax_session_id: None,
        qwen_session_id: None,
        codex_session_id: None,
        nvidia_session_id: None,
        gemini_session_id: None,
        openrouter_session_id: None,
        omniforge_session_id: None,
        omniforge_model_id: None,
        kimi_session_id: None,
        opencode_session_id: None,
        hermes_session_id: None,
        forge_session_id: None,
        claw_session_id: None,
        launched_from: None,
        created_at,
        updated_at: created_at + chrono::Duration::minutes(message_count.saturating_sub(1) as i64),
        messages,
        runsc_dirs: Vec::new(),
        runsc_image: None,
    }
}

#[test]
fn scroll_adjustment_is_deferred_while_user_is_scrolling() {
    assert_eq!(
        resolve_scroll_adjustment(12.0, 3.0, true, false),
        (None, 15.0)
    );
}

#[test]
fn deferred_scroll_adjustment_applies_once_scrolling_stops() {
    assert_eq!(
        resolve_scroll_adjustment(2.0, 3.5, false, false),
        (Some(5.5), 0.0)
    );
}

#[test]
fn scroll_adjustment_is_cleared_during_height_scan() {
    assert_eq!(
        resolve_scroll_adjustment(8.0, 4.0, false, true),
        (None, 0.0)
    );
}

#[test]
fn full_height_seed_runs_without_a_complete_cache() {
    let partial_entry = HeightCacheEntry {
        row_heights: vec![80.0, 90.0],
        total_height: 170.0,
        message_count: 2,
        updated_at: Utc::now(),
    };
    let full_entry = HeightCacheEntry {
        row_heights: vec![120.0; 15],
        total_height: 1800.0,
        message_count: 15,
        updated_at: Utc::now(),
    };

    assert!(should_seed_full_height_cache(None, 15, false));
    assert!(should_seed_full_height_cache(
        Some(&partial_entry),
        15,
        false
    ));
    assert!(!should_seed_full_height_cache(Some(&full_entry), 15, false));
}

#[test]
fn full_height_seed_skips_large_or_live_sessions() {
    assert!(!should_seed_full_height_cache(None, 0, false));
    assert!(!should_seed_full_height_cache(None, 201, false));
    assert!(!should_seed_full_height_cache(None, 15, true));
}

#[test]
fn pending_draft_shown_when_chat_matches_and_not_processing() {
    assert!(should_show_pending_draft(
        false,
        Some("chat-1"),
        Some("chat-1")
    ));
}

#[test]
fn pending_draft_hidden_when_processing() {
    assert!(!should_show_pending_draft(
        true,
        Some("chat-1"),
        Some("chat-1")
    ));
}

#[test]
fn pending_draft_hidden_when_chat_id_differs() {
    assert!(!should_show_pending_draft(
        false,
        Some("chat-1"),
        Some("chat-2")
    ));
}

#[test]
fn pending_draft_hidden_when_no_draft() {
    assert!(!should_show_pending_draft(false, None, Some("chat-1")));
}

#[test]
fn pending_draft_hidden_when_no_session() {
    assert!(!should_show_pending_draft(false, Some("chat-1"), None));
}

#[test]
fn stale_cache_with_new_measurements_schedules_save() {
    let entry = HeightCacheEntry {
        row_heights: vec![80.0, 90.0],
        total_height: 170.0,
        message_count: 2,
        updated_at: chrono::Utc::now(),
    };

    assert!(should_schedule_height_cache_save(
        Some(&entry),
        6,
        true,
        false
    ));
}

#[test]
fn complete_cache_with_new_measurements_does_not_schedule_save() {
    let entry = HeightCacheEntry {
        row_heights: vec![80.0, 90.0, 100.0, 110.0, 120.0, 130.0],
        total_height: 630.0,
        message_count: 6,
        updated_at: chrono::Utc::now(),
    };

    assert!(!should_schedule_height_cache_save(
        Some(&entry),
        6,
        true,
        false
    ));
}

#[test]
fn no_cache_with_new_measurements_does_not_schedule_save() {
    assert!(!should_schedule_height_cache_save(None, 6, true, false));
}

#[test]
fn processing_frame_does_not_schedule_stale_cache_save() {
    let entry = HeightCacheEntry {
        row_heights: vec![80.0, 90.0],
        total_height: 170.0,
        message_count: 2,
        updated_at: chrono::Utc::now(),
    };

    assert!(!should_schedule_height_cache_save(
        Some(&entry),
        6,
        true,
        true
    ));
}

#[test]
fn complete_cache_without_new_measurements_does_not_schedule_save() {
    let entry = HeightCacheEntry {
        row_heights: vec![80.0, 90.0, 100.0],
        total_height: 270.0,
        message_count: 3,
        updated_at: chrono::Utc::now(),
    };

    assert!(!should_schedule_height_cache_save(
        Some(&entry),
        3,
        false,
        false
    ));
}

use super::*;
use std::thread;
use std::time::{Duration, Instant};

fn wait_for_height_cache_load(state: &mut AppState, session_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while Instant::now() < deadline {
        state.drain_height_cache_load_results();
        if state.layout.session_height_cache.contains_key(session_id) {
            return;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for height cache load for {session_id}");
}

fn render_snapshot_for_state(
    state: &mut AppState,
    width: f32,
    height: f32,
) -> crate::app::ui::main_panel::MessageAreaSnapshot {
    let ctx = egui::Context::default();
    let mut raw_input = egui::RawInput::default();
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(width, height),
    ));

    let mut snapshot = None;
    let _ = ctx.run(raw_input, |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            snapshot = render_message_area(ui, state, true);
        });
    });

    snapshot.expect("render should capture a message area snapshot")
}

fn assert_snapshot_has_no_horizontal_overflow(
    snapshot: &crate::app::ui::main_panel::MessageAreaSnapshot,
) {
    assert!(
        snapshot.max_rendered_overflow <= 0.5,
        "rendered content overflowed horizontally: {}px",
        snapshot.max_rendered_overflow
    );
    for row in snapshot.message_rows.iter().filter(|row| row.rendered) {
        let message_rect = row
            .message_rect
            .expect("rendered row should include a message rect");
        assert!(
            message_rect.max.x <= snapshot.scroll_inner_rect.max.x + 0.5,
            "row {} exceeded the scroll viewport width: row_max_x={} viewport_max_x={}",
            row.index,
            message_rect.max.x,
            snapshot.scroll_inner_rect.max.x
        );
    }
}

#[test]
fn render_rebuilds_legacy_height_cache_without_saving_guesses() {
    let _home = TestHomeGuard::new();
    let _sync_save = SyncHeightCacheSaveGuard::enable();
    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).unwrap();
    let mut session = make_session("legacy-height-cache", 6);
    session.messages[1].content = "very tall assistant message\n".repeat(80);
    session.messages[3].content = "medium assistant message\n".repeat(30);
    session.messages[5].content = "short reply".to_string();
    data_manager.save_session(&session).unwrap();

    let width_key = height_cache_width_key(640.0);
    let mut poisoned_cache = SessionHeightCache {
        session_id: session.id.clone(),
        version: 1,
        widths: Default::default(),
    };
    poisoned_cache.widths.insert(
        width_key,
        HeightCacheEntry {
            row_heights: vec![120.0; session.messages.len()],
            total_height: 120.0 * session.messages.len() as f32,
            message_count: session.messages.len(),
            updated_at: Utc::now(),
        },
    );
    data_manager.save_height_cache(&poisoned_cache).unwrap();

    let mut state = AppState::new_for_tests(provider);
    state.session.data_manager = Some(DataManager::new(provider).unwrap());
    state.load_session(&session.id).unwrap();
    wait_for_height_cache_load(&mut state, &session.id);

    let cached_before = state
        .layout
        .session_height_cache
        .get(&session.id)
        .expect("legacy cache should load into state");
    assert_eq!(cached_before.version, 1);
    assert!(
        resolve_height_cache_entry(cached_before, width_key).is_none(),
        "legacy cache should be ignored during render"
    );

    let _snapshot = render_snapshot_for_state(&mut state, 640.0, 2400.0);
    assert!(state.layout.pending_height_cache_save.is_none());

    let rendered_width_key = height_cache_width_key(state.layout.message_row_heights_width);
    let persisted = DataManager::new(provider)
        .unwrap()
        .load_height_cache(&session.id)
        .unwrap()
        .expect("persisted height cache");
    assert_eq!(persisted.version, SESSION_HEIGHT_CACHE_VERSION);
    let updated_entry = persisted
        .widths
        .get(&rendered_width_key)
        .expect("rendered width bucket should be written");
    assert_eq!(updated_entry.message_count, session.messages.len());
    assert_eq!(updated_entry.row_heights.len(), session.messages.len());
    assert!(
        updated_entry
            .row_heights
            .iter()
            .any(|height| (*height - 120.0).abs() > 0.5),
        "rebuilt cache should contain measured heights instead of the poisoned placeholder average"
    );
    assert_ne!(
        updated_entry.total_height,
        120.0 * session.messages.len() as f32
    );
}

#[test]
fn render_with_pending_draft_does_not_clear_or_panic() {
    let provider = ModelProvider::Codex;
    let mut state = AppState::new_for_tests(provider);
    let session = make_session("draft-active", 3);
    state.session.current_session = Some(session.clone());
    state.session.selected_session_id = Some(session.id.clone());
    state.session.pending_draft = Some(PendingDraft {
        chat_id: session.id.clone(),
        user_message_timestamp_ms: 42,
    });

    let _snapshot = render_snapshot_for_state(&mut state, 640.0, 800.0);
    let draft =
        state.session.pending_draft.as_ref().expect(
            "pending_draft must persist across render — only stream/terminal events clear it",
        );
    assert_eq!(draft.chat_id, "draft-active");
    assert!(should_show_pending_draft(
        false,
        Some(draft.chat_id.as_str()),
        state
            .session
            .current_session
            .as_ref()
            .map(|s| s.id.as_str()),
    ));
}

#[test]
fn render_with_pending_draft_for_other_chat_hides_indicator() {
    let provider = ModelProvider::Codex;
    let mut state = AppState::new_for_tests(provider);
    let viewed = make_session("viewed-chat", 2);
    state.session.current_session = Some(viewed.clone());
    state.session.selected_session_id = Some(viewed.id.clone());
    state.session.pending_draft = Some(PendingDraft {
        chat_id: "other-chat".to_string(),
        user_message_timestamp_ms: 7,
    });

    let _snapshot = render_snapshot_for_state(&mut state, 640.0, 800.0);
    assert!(!should_show_pending_draft(
        false,
        state
            .session
            .pending_draft
            .as_ref()
            .map(|d| d.chat_id.as_str()),
        state
            .session
            .current_session
            .as_ref()
            .map(|s| s.id.as_str()),
    ));
    assert_eq!(
        state
            .session
            .pending_draft
            .as_ref()
            .map(|d| d.chat_id.as_str()),
        Some("other-chat")
    );
}

#[test]
fn optimistic_empty_area_matches_loaded_empty_chat_height() {
    let provider = ModelProvider::Codex;
    let mut loaded_state = AppState::new_for_tests(provider);
    loaded_state.session.current_session = Some(make_session("loaded-empty", 0));
    loaded_state.session.selected_session_id = Some("loaded-empty".to_string());

    let ctx = egui::Context::default();
    let mut raw_input = egui::RawInput::default();
    raw_input.screen_rect = Some(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(640.0, 800.0),
    ));
    let mut loaded_height = 0.0f32;
    let _ = ctx.run(raw_input.clone(), |ctx| {
        loaded_height = egui::CentralPanel::default()
            .show(ctx, |ui| {
                let start = ui.cursor().top();
                render_message_area(ui, &mut loaded_state, false);
                ui.cursor().top() - start
            })
            .inner;
    });

    let mut optimistic_height = 0.0f32;
    let _ = ctx.run(raw_input, |ctx| {
        optimistic_height = egui::CentralPanel::default()
            .show(ctx, |ui| {
                let start = ui.cursor().top();
                render_empty_message_area(ui, "pending-create");
                ui.cursor().top() - start
            })
            .inner;
    });

    assert!(
        (loaded_height - optimistic_height).abs() < 0.5,
        "optimistic empty transcript height must match loaded empty chat height: loaded={loaded_height}, optimistic={optimistic_height}"
    );
}

#[test]
fn render_creates_height_cache_when_none_exists_on_disk() {
    let _home = TestHomeGuard::new();
    let _sync_save = SyncHeightCacheSaveGuard::enable();
    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).unwrap();
    let mut session = make_session("no-cache-on-disk", 5);
    session.messages[1].content = "assistant response with several lines\n".repeat(20);
    session.messages[3].content = "another tall reply\n".repeat(40);
    data_manager.save_session(&session).unwrap();

    let mut state = AppState::new_for_tests(provider);
    state.session.data_manager = Some(DataManager::new(provider).unwrap());
    state.load_session(&session.id).unwrap();

    assert!(
        state.layout.session_height_cache.get(&session.id).is_none(),
        "no cache should exist before first render"
    );

    let _snapshot = render_snapshot_for_state(&mut state, 640.0, 2400.0);
    let rendered_width_key = height_cache_width_key(state.layout.message_row_heights_width);
    let persisted = DataManager::new(provider)
        .unwrap()
        .load_height_cache(&session.id)
        .unwrap()
        .expect("height cache should be created after render");
    assert_eq!(persisted.version, SESSION_HEIGHT_CACHE_VERSION);
    let entry = persisted
        .widths
        .get(&rendered_width_key)
        .expect("rendered width bucket should exist");
    assert_eq!(entry.message_count, session.messages.len());
    assert_eq!(entry.row_heights.len(), session.messages.len());
    assert!(
        entry.row_heights.iter().all(|h| *h > 0.0),
        "all cached heights must be positive measured values"
    );
}

#[test]
fn wrong_cached_heights_are_corrected_to_exact_values() {
    let _home = TestHomeGuard::new();
    let _sync_save = SyncHeightCacheSaveGuard::enable();
    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).unwrap();
    let mut session = make_session("exact-correction", 6);
    session.messages[0].content = "user question about something".to_string();
    session.messages[1].content = "medium assistant reply\n".repeat(8);
    session.messages[2].content = "follow-up question".to_string();
    session.messages[3].content = "longer explanation with detail\n".repeat(12);
    session.messages[4].content = "ok thanks".to_string();
    session.messages[5].content = "you're welcome, let me know if you need more help".to_string();
    data_manager.save_session(&session).unwrap();

    let mut state = AppState::new_for_tests(provider);
    state.session.data_manager = Some(DataManager::new(provider).unwrap());
    state.load_session(&session.id).unwrap();
    let _snapshot = render_snapshot_for_state(&mut state, 640.0, 2400.0);
    let exact_heights = state.layout.message_row_heights.clone();
    assert_eq!(exact_heights.len(), 6);

    let rendered_width_key = height_cache_width_key(state.layout.message_row_heights_width);
    let offsets = [25.0_f32, -22.0, 28.0, -20.0, 30.0, -27.0];
    let wrong_heights: Vec<f32> = exact_heights
        .iter()
        .zip(offsets.iter())
        .map(|(h, off)| (h + off).max(60.0))
        .collect();
    let mut corrupted_cache = SessionHeightCache {
        session_id: session.id.clone(),
        version: SESSION_HEIGHT_CACHE_VERSION,
        widths: Default::default(),
    };
    corrupted_cache.widths.insert(
        rendered_width_key,
        HeightCacheEntry {
            row_heights: wrong_heights.clone(),
            total_height: wrong_heights.iter().sum(),
            message_count: session.messages.len(),
            updated_at: Utc::now(),
        },
    );
    data_manager.save_height_cache(&corrupted_cache).unwrap();

    let mut state2 = AppState::new_for_tests(provider);
    state2.session.data_manager = Some(DataManager::new(provider).unwrap());
    state2.load_session(&session.id).unwrap();
    wait_for_height_cache_load(&mut state2, &session.id);

    let _snapshot = render_snapshot_for_state(&mut state2, 640.0, 2400.0);
    assert_eq!(state2.layout.message_row_heights.len(), exact_heights.len());
    for (index, (got, expected)) in state2
        .layout
        .message_row_heights
        .iter()
        .zip(exact_heights.iter())
        .enumerate()
    {
        assert!(
            (got - expected).abs() < 0.01,
            "row {index}: got {got}, expected {expected} — cached heights 20-30px off must be corrected to exact measured values"
        );
    }
}

#[test]
fn render_extends_stale_cache_with_fewer_messages() {
    let _home = TestHomeGuard::new();
    let _sync_save = SyncHeightCacheSaveGuard::enable();
    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).unwrap();
    let mut session = make_session("stale-message-count", 8);
    session.messages[1].content = "first assistant reply\n".repeat(10);
    session.messages[7].content = "newest message added after cache was built".to_string();
    data_manager.save_session(&session).unwrap();

    let width_key = height_cache_width_key(640.0);
    let mut stale_cache = SessionHeightCache {
        session_id: session.id.clone(),
        version: SESSION_HEIGHT_CACHE_VERSION,
        widths: Default::default(),
    };
    stale_cache.widths.insert(
        width_key,
        HeightCacheEntry {
            row_heights: vec![100.0; 5],
            total_height: 500.0,
            message_count: 5,
            updated_at: Utc::now(),
        },
    );
    data_manager.save_height_cache(&stale_cache).unwrap();

    let mut state = AppState::new_for_tests(provider);
    state.session.data_manager = Some(DataManager::new(provider).unwrap());
    state.load_session(&session.id).unwrap();
    wait_for_height_cache_load(&mut state, &session.id);

    let cached = state
        .layout
        .session_height_cache
        .get(&session.id)
        .expect("stale cache should load");
    assert!(
        !has_complete_height_cache_entry(
            resolve_height_cache_entry(cached, width_key),
            session.messages.len()
        ),
        "stale cache should not be considered complete"
    );

    let _snapshot = render_snapshot_for_state(&mut state, 640.0, 2400.0);
    let rendered_width_key = height_cache_width_key(state.layout.message_row_heights_width);
    let persisted = DataManager::new(provider)
        .unwrap()
        .load_height_cache(&session.id)
        .unwrap()
        .expect("height cache should persist");
    let entry = persisted
        .widths
        .get(&rendered_width_key)
        .expect("rendered width bucket should exist");
    assert_eq!(
        entry.message_count,
        session.messages.len(),
        "cache must cover all 8 messages after render"
    );
    assert_eq!(entry.row_heights.len(), session.messages.len());
}

#[test]
fn render_at_new_width_measures_fresh_without_reusing_distant_cache() {
    let _home = TestHomeGuard::new();
    let _sync_save = SyncHeightCacheSaveGuard::enable();
    let provider = ModelProvider::Codex;
    let data_manager = DataManager::new(provider).unwrap();
    let mut session = make_session("width-mismatch", 4);
    session.messages[1].content = "a reply that will be different width\n".repeat(25);
    data_manager.save_session(&session).unwrap();

    let old_width_key = height_cache_width_key(900.0);
    let mut old_cache = SessionHeightCache {
        session_id: session.id.clone(),
        version: SESSION_HEIGHT_CACHE_VERSION,
        widths: Default::default(),
    };
    old_cache.widths.insert(
        old_width_key,
        HeightCacheEntry {
            row_heights: vec![90.0, 200.0, 90.0, 90.0],
            total_height: 470.0,
            message_count: 4,
            updated_at: Utc::now(),
        },
    );
    data_manager.save_height_cache(&old_cache).unwrap();

    let mut state = AppState::new_for_tests(provider);
    state.session.data_manager = Some(DataManager::new(provider).unwrap());
    state.load_session(&session.id).unwrap();
    wait_for_height_cache_load(&mut state, &session.id);

    let new_width = 520.0;
    let new_width_key = height_cache_width_key(new_width);
    assert!(
        old_width_key.abs_diff(new_width_key) > 24,
        "widths must be outside the 24px tolerance"
    );

    let cached = state.layout.session_height_cache.get(&session.id).unwrap();
    assert!(
        resolve_height_cache_entry(cached, new_width_key).is_none(),
        "no cache entry should resolve for the distant width"
    );

    let _snapshot = render_snapshot_for_state(&mut state, new_width, 2400.0);
    let rendered_width_key = height_cache_width_key(state.layout.message_row_heights_width);
    let persisted = DataManager::new(provider)
        .unwrap()
        .load_height_cache(&session.id)
        .unwrap()
        .expect("height cache should persist");
    let entry = persisted
        .widths
        .get(&rendered_width_key)
        .expect("new width bucket should be created");
    assert_eq!(entry.message_count, session.messages.len());
    assert_eq!(entry.row_heights.len(), session.messages.len());
    assert!(
        persisted.widths.contains_key(&old_width_key),
        "original width entry should be preserved alongside the new one"
    );
}

#[test]
fn mixed_transcript_and_markdown_message_does_not_reintroduce_large_text_blocks() {
    let provider = ModelProvider::Codex;
    let mut state = AppState::new_for_tests(provider);
    let mut session = make_session("mixed-transcript-overflow", 2);
    session.messages[1].content = format!(
        "[May 19 01:28am] GPT-5.4 agent started\n[May 19 01:28am] GPT-5.4 | Bash `{}`\n[May 19 01:28am] I’ll inspect the transcript rendering path before editing.\n\n## Summary\n{}\n\n```json\n{{\"payload\":\"{}\"}}\n```",
        format!("python -c \"print('{}')\"", "x".repeat(220)),
        "This summary line stays intentionally long to exercise wrapping inside the message group without allowing the transcript body to widen the scroll content.".repeat(3),
        "y".repeat(260),
    );
    state.session.current_session = Some(session.clone());
    state.session.selected_session_id = Some(session.id.clone());

    let snapshot = render_snapshot_for_state(&mut state, 560.0, 1600.0);
    assert_snapshot_has_no_horizontal_overflow(&snapshot);
}

#[test]
fn plaintext_stream_message_does_not_reintroduce_large_text_blocks() {
    let provider = ModelProvider::Codex;
    let mut state = AppState::new_for_tests(provider);
    let mut session = make_session("plaintext-stream-overflow", 2);
    session.messages[1].content = format!(
        "● [03:11:37] Execute [/bin/bash] echo {}\n● [03:11:39] Read src/app/ui/message_list.rs\n{}\n```sh\n{}\n```",
        "x".repeat(220),
        "Tool transcript output stays readable even when the stream includes long lines and a large follow-up block.".repeat(4),
        format!("printf '%s\\n' '{}'", "z".repeat(260)),
    );
    state.session.current_session = Some(session.clone());
    state.session.selected_session_id = Some(session.id.clone());

    let snapshot = render_snapshot_for_state(&mut state, 560.0, 1600.0);
    assert_snapshot_has_no_horizontal_overflow(&snapshot);
}

#[test]
fn extracted_generic_tool_transcript_sample_does_not_overflow_or_hide_body_markdown() {
    let provider = ModelProvider::Codex;
    let mut state = AppState::new_for_tests(provider);
    let mut session = make_session("extracted-tool-transcript-sample", 2);
    session.messages[1].content =
        include_str!("../../../../../tests/fixtures/real_extracted_tool_transcript_sample.md")
            .to_string();
    state.session.current_session = Some(session.clone());
    state.session.selected_session_id = Some(session.id.clone());

    let snapshot = render_snapshot_for_state(&mut state, 560.0, 1400.0);
    assert_snapshot_has_no_horizontal_overflow(&snapshot);
}

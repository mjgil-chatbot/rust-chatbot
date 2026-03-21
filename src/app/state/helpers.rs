fn upsert_assistant_response(session: &mut ChatSession, response: &str) -> bool {
    let response = collapse_textdelta_accumulation(response);
    let response = strip_leading_think_block_response(&response);
    let response_before_preamble = response.clone();
    let response = preserved_forge_transcript_response(session.messages.last(), &response);
    if response.trim().is_empty() {
        return false;
    }
    if let Some(last) = session.messages.last_mut() {
        if !last.is_user {
            if last.content == response {
                return response_before_preamble != response;
            }
            last.content = response.to_string();
            session.updated_at = chrono::Utc::now();
            return true;
        }
    }

    session.add_message(response.to_string(), false);
    true
}

fn collapse_textdelta_accumulation(response: &str) -> String {
    let Some((preamble, separator, body)) = split_leading_transcript_preamble(response) else {
        return collapse_cumulative_prefixes(response).unwrap_or_else(|| response.to_string());
    };

    let Some(collapsed_body) = collapse_cumulative_prefixes(body) else {
        return response.to_string();
    };

    format!("{preamble}{separator}{collapsed_body}")
}

fn strip_leading_think_block_response(response: &str) -> String {
    let Some((preamble, separator, body)) = split_leading_transcript_preamble(response) else {
        return strip_leading_think_block(response).unwrap_or_else(|| response.to_string());
    };

    let Some(stripped_body) = strip_leading_think_block(body) else {
        return response.to_string();
    };
    if stripped_body.is_empty() {
        return preamble.trim_end_matches(['\r', '\n']).to_string();
    }

    format!("{preamble}{separator}{stripped_body}")
}

fn split_leading_transcript_preamble(response: &str) -> Option<(&str, &str, &str)> {
    let mut preamble_end = 0usize;
    let mut transcript_lines = 0usize;

    for segment in response.split_inclusive('\n') {
        let line = segment.strip_suffix('\n').unwrap_or(segment);
        let trimmed = line.trim_end_matches('\r').trim_start();
        if trimmed.is_empty() || !is_transcript_activity_line(trimmed) {
            break;
        }
        preamble_end += segment.len();
        transcript_lines += 1;
    }

    if transcript_lines == 0 || preamble_end >= response.len() {
        return None;
    }

    let mut body_start = preamble_end;
    while let Some(ch) = response[body_start..].chars().next() {
        if matches!(ch, '\n' | '\r') {
            body_start += ch.len_utf8();
        } else {
            break;
        }
    }

    if body_start >= response.len() {
        return None;
    }

    Some((
        &response[..preamble_end],
        &response[preamble_end..body_start],
        &response[body_start..],
    ))
}

fn is_transcript_activity_line(line: &str) -> bool {
    line.starts_with("● [")
        || line.starts_with("○ [")
        || line.starts_with("◦ [")
        || line.starts_with("• [")
        || strip_standard_transcript_timestamp_prefix(line).len() != line.len()
}

fn strip_standard_transcript_timestamp_prefix(line: &str) -> &str {
    let bytes = line.as_bytes();
    if bytes.len() < 16 || bytes[0] != b'[' {
        return line;
    }
    if !bytes[1].is_ascii_alphabetic()
        || !bytes[2].is_ascii_alphabetic()
        || !bytes[3].is_ascii_alphabetic()
        || bytes[4] != b' '
    {
        return line;
    }

    let mut index = 5usize;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 5 || index + 9 > bytes.len() || bytes[index] != b' ' {
        return line;
    }
    if !bytes[index + 1].is_ascii_digit()
        || !bytes[index + 2].is_ascii_digit()
        || bytes[index + 3] != b':'
        || !bytes[index + 4].is_ascii_digit()
        || !bytes[index + 5].is_ascii_digit()
        || !bytes[index + 6].is_ascii_lowercase()
        || !bytes[index + 7].is_ascii_lowercase()
        || bytes[index + 8] != b']'
        || index + 9 >= bytes.len()
        || bytes[index + 9] != b' '
    {
        return line;
    }

    &line[index + 10..]
}

fn collapse_cumulative_prefixes(content: &str) -> Option<String> {
    if content.len() < 24 {
        return None;
    }

    let boundaries = content
        .char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(content.len()))
        .collect::<Vec<_>>();

    for &start in boundaries.iter().skip(1) {
        let candidate = &content[start..];
        let prefix_boundaries = candidate
            .char_indices()
            .map(|(idx, _)| idx)
            .skip(1)
            .chain(std::iter::once(candidate.len()))
            .collect::<Vec<_>>();
        let mut offset = 0usize;
        let mut previous_prefix_len = 0usize;
        let mut segment_count = 0usize;
        let mut valid = true;

        while offset < content.len() {
            let remaining = &content[offset..];
            let mut matched_len = 0usize;

            for &prefix_len in &prefix_boundaries {
                if remaining.starts_with(&candidate[..prefix_len]) {
                    matched_len = prefix_len;
                } else {
                    break;
                }
            }

            if matched_len == 0 || matched_len <= previous_prefix_len {
                valid = false;
                break;
            }

            previous_prefix_len = matched_len;
            offset += matched_len;
            segment_count += 1;
        }

        if valid && offset == content.len() && segment_count >= 3 {
            return Some(candidate.to_string());
        }
    }

    None
}

fn strip_leading_think_block(content: &str) -> Option<String> {
    let trimmed = content.trim_start();
    let think_open = "<think>";
    let think_close = "</think>";
    if !trimmed.starts_with(think_open) {
        return None;
    }

    let close_index = trimmed.find(think_close)?;
    let remainder =
        trimmed[close_index + think_close.len()..].trim_start_matches(['\r', '\n', ' ']);
    Some(remainder.to_string())
}

fn preserved_forge_transcript_response(
    previous_message: Option<&crate::data::ChatMessage>,
    response: &str,
) -> String {
    let Some(previous_content) = previous_message
        .filter(|message| !message.is_user)
        .map(|message| message.content.as_str())
    else {
        return response.to_string();
    };

    let Some(previous_preamble) = crate::forge_loop::transcript_preamble(previous_content) else {
        return response.to_string();
    };

    let normalized_response = crate::forge_loop::sanitize_stream_plaintext(response);
    if normalized_response.is_empty() {
        return response.to_string();
    }
    if crate::forge_loop::transcript_preamble(&normalized_response).is_some() {
        return normalized_response;
    }
    if normalized_response == previous_content {
        return normalized_response;
    }

    format!("{previous_preamble}\n{normalized_response}")
}

fn append_terminal_message_once(session: &mut ChatSession, content: &str) {
    let already_present = session
        .messages
        .last()
        .map(|last| !last.is_user && last.content == content)
        .unwrap_or(false);
    if !already_present {
        session.add_message(content.to_string(), false);
    }
}

fn replace_provider_session_id(
    session: &mut ChatSession,
    provider: ModelProvider,
    session_id: Option<&str>,
) {
    let Some(session_id) = session_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    provider.set_session_id_on_chat(session, session_id.to_string());
}

fn session_transcript_is_strict_prefix(current: &ChatSession, loaded: &ChatSession) -> bool {
    loaded.messages.len() > current.messages.len()
        && current.id == loaded.id
        && current.messages.iter().zip(&loaded.messages).all(|(a, b)| {
            a.content == b.content
                && a.is_user == b.is_user
                && a.timestamp == b.timestamp
                && a.command == b.command
                && a.retrieved_memory == b.retrieved_memory
        })
}

fn session_reload_appends_user_turn(current: &ChatSession, loaded: &ChatSession) -> bool {
    if current.id != loaded.id
        || current.messages.is_empty()
        || loaded.messages.len() != current.messages.len() + 1
        || current
            .messages
            .last()
            .is_none_or(|message| message.is_user)
        || loaded
            .messages
            .last()
            .is_none_or(|message| !message.is_user)
    {
        return false;
    }

    let stable_prefix_len = current.messages.len().saturating_sub(1);
    let stable_prefix_matches = current.messages[..stable_prefix_len]
        .iter()
        .zip(&loaded.messages[..stable_prefix_len])
        .all(|(a, b)| {
            a.content == b.content
                && a.is_user == b.is_user
                && a.timestamp == b.timestamp
                && a.command == b.command
                && a.retrieved_memory == b.retrieved_memory
        });
    if !stable_prefix_matches {
        return false;
    }

    let current_tail = &current.messages[stable_prefix_len];
    let loaded_tail = &loaded.messages[stable_prefix_len];
    !current_tail.is_user
        && !loaded_tail.is_user
        && current_tail.timestamp == loaded_tail.timestamp
        && current_tail.command == loaded_tail.command
        && current_tail.retrieved_memory == loaded_tail.retrieved_memory
}

fn session_transcripts_match(current: &ChatSession, loaded: &ChatSession) -> bool {
    current.id == loaded.id
        && current.messages.len() == loaded.messages.len()
        && current.messages.iter().zip(&loaded.messages).all(|(a, b)| {
            a.content == b.content
                && a.is_user == b.is_user
                && a.timestamp == b.timestamp
                && a.command == b.command
                && a.retrieved_memory == b.retrieved_memory
        })
}

fn session_reload_is_safe(current: &ChatSession, loaded: &ChatSession) -> bool {
    session_transcript_is_strict_prefix(current, loaded)
        || (session_transcripts_match(current, loaded) && loaded.updated_at >= current.updated_at)
}

fn merge_provider_update(
    existing: &mut JsonSummary,
    new_summary: &JsonSummary,
    provider: ModelProvider,
) {
    if provider == ModelProvider::Claude
        || provider == ModelProvider::MiniMax
        || provider == ModelProvider::MiniMaxForge
    {
        // Record per-call usage before merging.
        existing.record_turn_usage(new_summary);

        // duration_ms and token counts from Claude's result event
        // are per-request, so sum them across turns.
        existing.total_duration_ms += new_summary.total_duration_ms;
        existing.total_input_tokens += new_summary.total_input_tokens;
        existing.total_output_tokens += new_summary.total_output_tokens;
        existing.cache_creation_tokens += new_summary.cache_creation_tokens;
        existing.cache_read_tokens += new_summary.cache_read_tokens;
        existing.total_cost_usd += new_summary.total_cost_usd;
        // num_turns from Claude is per-invocation; sum across turns.
        existing.response_count += new_summary.response_count;
        if new_summary.context_window_tokens > 0 {
            // Detect compaction: context tokens decreased significantly.
            if existing.context_window_tokens > 0
                && new_summary.context_window_tokens < existing.context_window_tokens / 2
            {
                existing.num_compactions += 1;
            }
            existing.context_window_tokens = new_summary.context_window_tokens;
        }
        merge_summary_metadata(existing, new_summary);
    } else if provider == ModelProvider::Codex {
        let turn_delta = codex_turn_delta(existing, new_summary);
        existing.record_turn_usage(&turn_delta);

        // Codex reports cumulative session totals, so keep the latest
        // snapshot for totals and record only the delta in turn history.
        existing.total_duration_ms += turn_delta.total_duration_ms;
        existing.total_input_tokens = new_summary.total_input_tokens;
        existing.total_output_tokens = new_summary.total_output_tokens;
        existing.cache_creation_tokens = new_summary.cache_creation_tokens;
        existing.cache_read_tokens = new_summary.cache_read_tokens;
        existing.total_cost_usd = new_summary.total_cost_usd;
        existing.response_count += new_summary.response_count;
        existing.context_window_tokens = new_summary.context_window_tokens;
        existing.num_compactions += new_summary.num_compactions;
        merge_summary_metadata(existing, new_summary);
    } else {
        // Record per-call usage before merging.
        existing.record_turn_usage(new_summary);
        existing.merge(new_summary);
    }
}

fn from_provider_update(new_summary: &JsonSummary, provider: ModelProvider) -> JsonSummary {
    let mut first = new_summary.clone();
    if provider == ModelProvider::Codex {
        let turn_delta = codex_turn_delta(&JsonSummary::new(), new_summary);
        first.record_turn_usage(&turn_delta);
    } else {
        first.record_turn_usage(new_summary);
    }
    first
}

fn apply_stream_response(session: &mut ChatSession, response: &str) -> bool {
    let normalized = crate::forge_loop::sanitize_stream_plaintext(response);
    if normalized.is_empty() && !response.trim().is_empty() {
        return false;
    }
    let response = if normalized != response {
        normalized.as_str()
    } else {
        response
    };
    upsert_assistant_response(session, response)
}

fn merge_session_summary(
    summary: &mut Option<JsonSummary>,
    new_summary: Option<&JsonSummary>,
    provider: ModelProvider,
) {
    let Some(new_summary) = new_summary else {
        return;
    };

    match summary {
        Some(existing) => merge_provider_update(existing, new_summary, provider),
        None => *summary = Some(from_provider_update(new_summary, provider)),
    }
}

fn codex_turn_delta(existing: &JsonSummary, cumulative: &JsonSummary) -> JsonSummary {
    let mut turn = cumulative.clone();
    turn.total_input_tokens = cumulative
        .total_input_tokens
        .saturating_sub(existing.total_input_tokens);
    turn.total_output_tokens = cumulative
        .total_output_tokens
        .saturating_sub(existing.total_output_tokens);
    turn.cache_creation_tokens = cumulative
        .cache_creation_tokens
        .saturating_sub(existing.cache_creation_tokens);
    turn.cache_read_tokens = cumulative
        .cache_read_tokens
        .saturating_sub(existing.cache_read_tokens);
    turn.total_cost_usd = (cumulative.total_cost_usd - existing.total_cost_usd).max(0.0);
    turn.context_window_tokens = 0;

    for tool_call in &mut turn.pending_tool_calls {
        tool_call.input_tokens = turn.total_input_tokens;
        tool_call.output_tokens = turn.total_output_tokens;
        tool_call.cache_creation_tokens = turn.cache_creation_tokens;
        tool_call.cache_read_tokens = turn.cache_read_tokens;
    }

    turn
}

fn merge_summary_metadata(existing: &mut JsonSummary, new_summary: &JsonSummary) {
    if new_summary.context_window_size > 0 {
        existing.context_window_size = new_summary.context_window_size;
    }
    if existing.session_id.is_none() {
        existing.session_id = new_summary.session_id.clone();
    }
    if existing.model.is_none() {
        existing.model = new_summary.model.clone();
    }
    if existing.reasoning_effort.is_none() {
        existing.reasoning_effort = new_summary.reasoning_effort.clone();
    }
    for tool in &new_summary.tools_used {
        if !existing.tools_used.contains(tool) {
            existing.tools_used.push(tool.clone());
        }
    }
}

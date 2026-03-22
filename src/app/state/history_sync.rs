use std::collections::HashSet;
use std::path::Path;

use ai_history_core::{BranchId, SessionId, TurnId};
use ai_history_store::{HistoryStore as SqliteStore, MessageRecord};
use anyhow::Result;

use super::{auto_turn_checkpoint_reason, create_turn_checkpoint, ChatSession, ParsedTurn};

pub(super) fn reconcile_existing_turn(
    store: &SqliteStore,
    turn_id: TurnId,
    parsed: &ParsedTurn,
) -> Result<bool> {
    if parsed.assistant_content.is_none() {
        return Ok(false);
    }

    let messages = store.get_messages_for_turn(turn_id)?;
    let has_assistant = messages.iter().any(|message| message.role == "assistant");
    if has_assistant {
        return Ok(false);
    }

    let assistant = MessageRecord::new(
        turn_id,
        "Assistant",
        "assistant",
        parsed.assistant_content.as_deref().unwrap_or_default(),
        messages.len() as u64,
    );
    store.create_message(&assistant)?;
    store.finalize_turn(turn_id)?;
    Ok(true)
}

pub(super) fn backfill_missing_turn_checkpoints(
    store: &mut SqliteStore,
    workspace_root: &Path,
    session_id: SessionId,
    branch_id: BranchId,
    chat: &ChatSession,
    parsed_turns: &[ParsedTurn],
    checkpointed_turns: &mut HashSet<TurnId>,
) -> Result<()> {
    for (index, turn) in store
        .get_turns_for_branch(branch_id)?
        .into_iter()
        .enumerate()
    {
        let Some(parsed) = parsed_turns.get(index) else {
            break;
        };
        if parsed.assistant_content.is_none() || checkpointed_turns.contains(&turn.id) {
            continue;
        }
        let has_assistant = store
            .get_messages_for_turn(turn.id)?
            .iter()
            .any(|message| message.role == "assistant");
        if !has_assistant {
            continue;
        }
        create_turn_checkpoint(
            store,
            workspace_root,
            session_id,
            branch_id,
            turn.id,
            auto_turn_checkpoint_reason(chat, index),
        )?;
        checkpointed_turns.insert(turn.id);
    }
    Ok(())
}

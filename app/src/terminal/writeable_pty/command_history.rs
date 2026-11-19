use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use parking_lot::FairMutex;
use warpui::{AppContext, ModelHandle, SingletonEntity};

use crate::persistence::{ModelEvent, StartedCommandMetadata};
use crate::terminal::model::block::BlockId;
use crate::terminal::model::session::Sessions;
use crate::terminal::model::session::command_executor::is_in_band_command;
use crate::terminal::view::ExecuteCommandEvent;
use crate::terminal::{History, HistoryEntry, TerminalModel};

pub fn update_command_history(
    event: &ExecuteCommandEvent,
    model: &Arc<FairMutex<TerminalModel>>,
    model_event_sender: Option<&SyncSender<ModelEvent>>,
    sessions: &ModelHandle<Sessions>,
    ctx: &mut AppContext,
) {
    if is_in_band_command(&event.command) {
        return;
    }

    let model = model.lock();
    let active_block = model.block_list().active_block();
    let session_id = event.session_id;
    let Some(session) = sessions.as_ref(ctx).get(session_id) else {
        return;
    };

    let shell = session.shell();
    if !shell.should_add_command_to_history(&event.command) {
        return;
    }

    let is_agent_executed = event.source.is_ai_command();

    let session_ref = &*session;
    History::handle(ctx).update(ctx, move |history, _| {
        history.append_commands(
            session_id,
            vec![HistoryEntry::for_session_command(
                event.command.to_string(),
                active_block,
                session_ref,
                event.workflow_id.to_owned(),
                event.workflow_command.to_owned(),
                is_agent_executed,
            )],
        );
    });

    if let Some(sender) = model_event_sender {
        let sender_clone = sender.clone();
        let insert_command_event = ModelEvent::InsertCommand {
            metadata: StartedCommandMetadata {
                command: event.command.to_owned(),
                start_ts: active_block.start_ts().copied(),
                pwd: active_block.pwd().map(|pwd| pwd.to_owned()),
                shell: Some(session.shell().shell_type().name().to_owned()),
                username: Some(session.user().to_owned()),
                hostname: Some(session.hostname().to_owned()),
                session_id: Some(session_id),
                cloud_workflow_id: event.workflow_id.to_owned(),
                workflow_command: event.workflow_command.to_owned(),
                git_branch: active_block
                    .git_branch()
                    .map(|git_branch| git_branch.to_owned()),
                is_agent_executed,
            },
        };
        ctx.background_executor()
            .spawn(async move {
                // Sending over a sync sender can block the current thread, so we do this async.
                if let Err(e) = sender_clone.send(insert_command_event) {
                    log::warn!("Error sending ModelEvent: {e:#}");
                }
            })
            .detach();
    }
}

/// Updates the history entry for `block_id` when the shell ran something other than what was
/// typed, such as `!!` expanded to the previous command. History records the typed text on Enter,
/// before the shell has expanded it.
pub fn record_shell_expanded_command(
    block_id: &BlockId,
    typed: &str,
    shell_reported: &str,
    model: &Arc<FairMutex<TerminalModel>>,
    sessions: &ModelHandle<Sessions>,
    ctx: &mut AppContext,
) {
    // Look the block up by ID: a fast command may have finished already, so the active block may
    // not be the one this event is about.
    let Some((session_id, start_ts)) = model
        .lock()
        .block_list()
        .block_with_id(block_id)
        .and_then(|block| Some((block.session_id()?, block.start_ts().copied()?)))
    else {
        return;
    };
    let Some(shell_type) = sessions
        .as_ref(ctx)
        .get(session_id)
        .map(|session| session.shell().shell_type())
    else {
        return;
    };
    let Some(expanded) =
        History::shell_expanded_command(typed, shell_reported, shell_type).map(str::to_owned)
    else {
        return;
    };

    History::handle(ctx).update(ctx, |history, _| {
        history.replace_command(session_id, start_ts, typed, expanded);
    });
}

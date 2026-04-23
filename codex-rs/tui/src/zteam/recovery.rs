use super::WorkerSlot;
use super::preview;
use super::worker_source::WorkerSource;
use super::worker_source::local_thread_matches_slot;
use codex_app_server_protocol::SessionSource;
use codex_app_server_protocol::Thread;
use codex_app_server_protocol::ThreadItem;
use codex_app_server_protocol::ThreadStatus;
use codex_app_server_protocol::Turn;
use codex_app_server_protocol::UserInput;
use codex_protocol::ThreadId;
use codex_protocol::models::MessagePhase;
use codex_protocol::protocol::SubAgentSource;
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) enum WorkerConnection {
    #[default]
    Pending,
    Live(ThreadId),
    ReattachRequired(ThreadId),
}

impl WorkerConnection {
    pub(crate) fn live_thread_id(&self) -> Option<ThreadId> {
        match self {
            Self::Live(thread_id) => Some(*thread_id),
            Self::Pending | Self::ReattachRequired(_) => None,
        }
    }

    pub(crate) fn known_thread_id(&self) -> Option<ThreadId> {
        match self {
            Self::Pending => None,
            Self::Live(thread_id) | Self::ReattachRequired(thread_id) => Some(*thread_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecoveredWorker {
    pub(crate) slot: WorkerSlot,
    pub(crate) connection: WorkerConnection,
    pub(crate) source: WorkerSource,
    pub(crate) last_dispatched_task: Option<String>,
    pub(crate) last_result: Option<String>,
}

pub(crate) fn latest_local_threads_for_primary(
    threads: Vec<Thread>,
    primary_thread_id: ThreadId,
) -> Vec<Thread> {
    let descendants = descendant_threads(threads, primary_thread_id);
    let mut latest = BTreeMap::new();
    for thread in descendants {
        let Some(slot) = WorkerSlot::ALL
            .into_iter()
            .find(|slot| local_thread_matches_slot(*slot, &thread))
        else {
            continue;
        };
        let should_replace = latest
            .get(&slot)
            .is_none_or(|existing: &Thread| existing.updated_at <= thread.updated_at);
        if should_replace {
            latest.insert(slot, thread);
        }
    }

    WorkerSlot::ALL
        .into_iter()
        .filter_map(|slot| latest.remove(&slot))
        .collect()
}

pub(crate) fn recover_local_worker(
    thread: &Thread,
    live_connection: bool,
) -> Option<RecoveredWorker> {
    let slot = WorkerSlot::ALL
        .into_iter()
        .find(|slot| local_thread_matches_slot(*slot, thread))?;
    let thread_id = ThreadId::from_string(&thread.id).ok()?;
    let connection = if live_connection && !matches!(thread.status, ThreadStatus::NotLoaded) {
        WorkerConnection::Live(thread_id)
    } else {
        WorkerConnection::ReattachRequired(thread_id)
    };
    let (last_dispatched_task, last_result) = summarize_turns(&thread.turns, &thread.preview);
    Some(RecoveredWorker {
        slot,
        connection,
        source: WorkerSource::LocalThreadSpawn,
        last_dispatched_task,
        last_result,
    })
}

fn descendant_threads(threads: Vec<Thread>, primary_thread_id: ThreadId) -> Vec<Thread> {
    let mut threads_by_id = HashMap::new();
    for thread in threads {
        let Ok(thread_id) = ThreadId::from_string(&thread.id) else {
            continue;
        };
        threads_by_id.insert(thread_id, thread);
    }

    let mut included = HashSet::new();
    let mut pending = vec![primary_thread_id];
    while let Some(parent_thread_id) = pending.pop() {
        for (thread_id, thread) in &threads_by_id {
            if included.contains(thread_id) {
                continue;
            }

            let SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id: source_parent_thread_id,
                ..
            }) = &thread.source
            else {
                continue;
            };

            if *source_parent_thread_id != parent_thread_id {
                continue;
            }

            included.insert(*thread_id);
            pending.push(*thread_id);
        }
    }

    included
        .into_iter()
        .filter_map(|thread_id| threads_by_id.remove(&thread_id))
        .collect()
}

fn summarize_turns(turns: &[Turn], thread_preview: &str) -> (Option<String>, Option<String>) {
    let mut last_dispatched_task = None;
    let mut last_result = None;

    for turn in turns {
        for item in &turn.items {
            match item {
                ThreadItem::UserMessage { content, .. } => {
                    let text = content
                        .iter()
                        .filter_map(extract_user_input_text)
                        .collect::<Vec<_>>()
                        .join(" ");
                    if !text.trim().is_empty() {
                        last_dispatched_task = Some(preview(&text));
                    }
                }
                ThreadItem::AgentMessage { text, phase, .. }
                    if !text.trim().is_empty() && *phase != Some(MessagePhase::Commentary) =>
                {
                    last_result = Some(preview(text));
                }
                _ => {}
            }
        }
    }

    if last_result.is_none() && !thread_preview.trim().is_empty() {
        last_result = Some(preview(thread_preview));
    }

    (last_dispatched_task, last_result)
}

fn extract_user_input_text(input: &UserInput) -> Option<String> {
    match input {
        UserInput::Text { text, .. } if !text.trim().is_empty() => Some(text.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::RecoveredWorker;
    use super::WorkerConnection;
    use super::latest_local_threads_for_primary;
    use super::recover_local_worker;
    use crate::zteam::WorkerSlot;
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadItem;
    use codex_app_server_protocol::ThreadStatus;
    use codex_app_server_protocol::Turn;
    use codex_app_server_protocol::UserInput;
    use codex_protocol::ThreadId;
    use codex_protocol::models::MessagePhase;
    use codex_protocol::protocol::SubAgentSource;
    use codex_utils_absolute_path::test_support::PathBufExt;
    use codex_utils_absolute_path::test_support::test_path_buf;
    use pretty_assertions::assert_eq;

    fn test_thread(
        thread_id: ThreadId,
        parent_thread_id: ThreadId,
        slot: WorkerSlot,
        status: ThreadStatus,
        updated_at: i64,
    ) -> Thread {
        Thread {
            id: thread_id.to_string(),
            forked_from_id: None,
            preview: String::new(),
            ephemeral: false,
            model_provider: "openai".to_string(),
            created_at: updated_at,
            updated_at,
            status,
            path: None,
            cwd: test_path_buf("/tmp").abs(),
            cli_version: "0.0.0".to_string(),
            source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
                parent_thread_id,
                depth: 1,
                parent_model: None,
                agent_path: Some(format!("/root/{}", slot.task_name()).parse().expect("path")),
                agent_nickname: Some(slot.display_name().to_string()),
                agent_role: Some(slot.role_name().to_string()),
            }),
            agent_nickname: Some(slot.display_name().to_string()),
            agent_role: Some(slot.role_name().to_string()),
            git_info: None,
            name: None,
            turns: Vec::new(),
        }
    }

    #[test]
    fn latest_local_threads_for_primary_keeps_newest_thread_per_slot() {
        let primary_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000001").expect("valid thread");
        let older_frontend =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let newer_frontend =
            ThreadId::from_string("00000000-0000-0000-0000-000000000011").expect("valid thread");
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let unrelated_parent =
            ThreadId::from_string("00000000-0000-0000-0000-000000000099").expect("valid thread");
        let unrelated_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000100").expect("valid thread");

        let latest = latest_local_threads_for_primary(
            vec![
                test_thread(
                    older_frontend,
                    primary_thread_id,
                    WorkerSlot::Frontend,
                    ThreadStatus::Idle,
                    10,
                ),
                test_thread(
                    newer_frontend,
                    primary_thread_id,
                    WorkerSlot::Frontend,
                    ThreadStatus::Idle,
                    20,
                ),
                test_thread(
                    backend_id,
                    primary_thread_id,
                    WorkerSlot::Backend,
                    ThreadStatus::Idle,
                    15,
                ),
                test_thread(
                    unrelated_id,
                    unrelated_parent,
                    WorkerSlot::Backend,
                    ThreadStatus::Idle,
                    30,
                ),
            ],
            primary_thread_id,
        );

        let latest_ids: Vec<_> = latest.into_iter().map(|thread| thread.id).collect();
        assert_eq!(
            latest_ids,
            vec![newer_frontend.to_string(), backend_id.to_string()]
        );
    }

    #[test]
    fn recover_local_worker_prefers_turn_summaries_and_marks_not_loaded_threads() {
        let primary_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000001").expect("valid thread");
        let backend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
        let mut thread = test_thread(
            backend_id,
            primary_thread_id,
            WorkerSlot::Backend,
            ThreadStatus::NotLoaded,
            15,
        );
        thread.preview = "预览结果".to_string();
        thread.turns = vec![Turn {
            id: "turn-1".to_string(),
            items: vec![
                ThreadItem::UserMessage {
                    id: "user-1".to_string(),
                    content: vec![UserInput::Text {
                        text: "整理接口字段".to_string(),
                        text_elements: Vec::new(),
                    }],
                },
                ThreadItem::AgentMessage {
                    id: "msg-1".to_string(),
                    text: "后端阶段结果：接口字段已统一。".to_string(),
                    phase: Some(MessagePhase::FinalAnswer),
                    memory_citation: None,
                },
            ],
            status: codex_app_server_protocol::TurnStatus::Completed,
            error: None,
            started_at: None,
            completed_at: None,
            duration_ms: None,
        }];

        let recovered = recover_local_worker(&thread, /*live_connection*/ false);

        assert_eq!(
            recovered,
            Some(RecoveredWorker {
                slot: WorkerSlot::Backend,
                connection: WorkerConnection::ReattachRequired(backend_id),
                source: crate::zteam::WorkerSource::LocalThreadSpawn,
                last_dispatched_task: Some("整理接口字段".to_string()),
                last_result: Some("后端阶段结果：接口字段已统一。".to_string()),
            })
        );
    }

    #[test]
    fn recover_local_worker_marks_live_only_when_explicitly_allowed() {
        let primary_thread_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000001").expect("valid thread");
        let frontend_id =
            ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
        let thread = test_thread(
            frontend_id,
            primary_thread_id,
            WorkerSlot::Frontend,
            ThreadStatus::Idle,
            10,
        );

        let recovered = recover_local_worker(&thread, /*live_connection*/ true);

        assert_eq!(
            recovered,
            Some(RecoveredWorker {
                slot: WorkerSlot::Frontend,
                connection: WorkerConnection::Live(frontend_id),
                source: crate::zteam::WorkerSource::LocalThreadSpawn,
                last_dispatched_task: None,
                last_result: None,
            })
        );
    }
}

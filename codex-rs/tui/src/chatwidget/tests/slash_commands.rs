use super::*;
use crate::legacy_core::DEFAULT_AGENTS_MD_FILENAME;
use pretty_assertions::assert_eq;

fn normalize_rendered_text(text: &str) -> String {
    text.chars().filter(|ch| !ch.is_whitespace()).collect()
}

#[tokio::test]
async fn buddy_is_visible_by_default() {
    let (chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    assert!(chat.bottom_pane.buddy_visible());
}

#[tokio::test]
async fn buddy_stays_hidden_when_config_disables_visibility() {
    let mut config = test_config().await;
    config.tui_show_buddy = false;

    let (chat, _rx, _op_rx) =
        make_chatwidget_manual_with_config(config, /*model_override*/ None).await;

    assert!(!chat.bottom_pane.buddy_visible());
}

fn submit_composer_text(chat: &mut ChatWidget, text: &str) {
    chat.bottom_pane
        .set_composer_text(text.to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
}

fn recall_latest_after_clearing(chat: &mut ChatWidget) -> String {
    chat.bottom_pane
        .set_composer_text(String::new(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    chat.bottom_pane.composer_text()
}

#[tokio::test]
async fn slash_compact_eagerly_queues_follow_up_before_turn_start() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Compact);

    assert!(chat.bottom_pane.is_task_running());
    match rx.try_recv() {
        Ok(AppEvent::CodexOp(Op::Compact)) => {}
        other => panic!("expected compact op to be submitted, got {other:?}"),
    }

    chat.bottom_pane.set_composer_text(
        "queued before compact turn start".to_string(),
        Vec::new(),
        Vec::new(),
    );
    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert!(chat.pending_steers.is_empty());
    assert_eq!(chat.queued_user_messages.len(), 1);
    assert_eq!(
        chat.queued_user_messages.front().unwrap().text,
        "queued before compact turn start"
    );
    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn ctrl_d_quits_without_prompt() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_matches!(rx.try_recv(), Ok(AppEvent::Exit(ExitMode::ShutdownFirst)));
}

#[tokio::test]
async fn ctrl_d_with_modal_open_does_not_quit() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.open_approvals_popup();
    chat.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));

    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn slash_init_skips_when_project_doc_exists() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let tempdir = tempdir().unwrap();
    let existing_path = tempdir.path().join(DEFAULT_AGENTS_MD_FILENAME);
    std::fs::write(&existing_path, "existing instructions").unwrap();
    chat.config.cwd = tempdir.path().to_path_buf().abs();

    submit_composer_text(&mut chat, "/init");

    match op_rx.try_recv() {
        Err(TryRecvError::Empty) => {}
        other => panic!("expected no Codex op to be sent, got {other:?}"),
    }

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains(DEFAULT_AGENTS_MD_FILENAME),
        "info message should mention the existing file: {rendered:?}"
    );
    assert!(
        rendered.contains("已跳过 /init"),
        "info message should explain why /init was skipped: {rendered:?}"
    );
    assert_eq!(
        std::fs::read_to_string(existing_path).unwrap(),
        "existing instructions"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/init");
}

#[tokio::test]
async fn bare_slash_command_is_available_from_local_recall_after_dispatch() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/diff");

    let _ = drain_insert_history(&mut rx);
    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(chat.bottom_pane.composer_text(), "/diff");
}

#[tokio::test]
async fn inline_slash_command_is_available_from_local_recall_after_dispatch() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/rename Better title");

    let _ = drain_insert_history(&mut rx);
    chat.handle_key_event(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE));
    assert_eq!(chat.bottom_pane.composer_text(), "/rename Better title");
}

#[tokio::test]
async fn slash_rename_prefills_existing_thread_name() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_name = Some("Current project title".to_string());

    chat.dispatch_command(SlashCommand::Rename);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert_chatwidget_snapshot!("slash_rename_prefilled_prompt", popup);

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::CodexOp(Op::SetThreadName { name })) if name == "Current project title"
    );
}

#[tokio::test]
async fn slash_rename_without_existing_thread_name_starts_empty() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Rename);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    let normalized_popup = normalize_rendered_text(&popup);
    assert!(normalized_popup.contains("命名线程"));
    assert!(normalized_popup.contains("输入名称后按Enter"));

    chat.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn usage_error_slash_command_is_available_from_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.3-codex")).await;
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    submit_composer_text(&mut chat, "/fast maybe");

    assert_eq!(chat.bottom_pane.composer_text(), "");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("用法：/fast [on|off|status]"),
        "expected usage message, got: {rendered:?}"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/fast maybe");
}

#[tokio::test]
async fn unrecognized_slash_command_is_not_added_to_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/does-not-exist");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("无法识别命令 '/does-not-exist'"),
        "expected unrecognized-command message, got: {rendered:?}"
    );
    assert_eq!(chat.bottom_pane.composer_text(), "/does-not-exist");
    assert_eq!(recall_latest_after_clearing(&mut chat), "");
}

#[tokio::test]
async fn unavailable_slash_command_is_available_from_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.bottom_pane.set_task_running(/*running*/ true);

    submit_composer_text(&mut chat, "/model");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("'/model' 在任务进行中被禁用。"),
        "expected disabled-command message, got: {rendered:?}"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/model");
}

#[tokio::test]
async fn no_op_stub_slash_command_is_available_from_local_recall() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    submit_composer_text(&mut chat, "/debug-m-drop");

    let cells = drain_insert_history(&mut rx);
    let rendered = cells
        .iter()
        .map(|cell| lines_to_single_string(cell))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("记忆维护"),
        "expected stub message, got: {rendered:?}"
    );
    assert_eq!(recall_latest_after_clearing(&mut chat), "/debug-m-drop");
}

#[tokio::test]
async fn slash_quit_requests_exit() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Quit);

    assert_matches!(rx.try_recv(), Ok(AppEvent::Exit(ExitMode::ShutdownFirst)));
}

#[tokio::test]
async fn slash_logout_requests_app_server_logout() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Logout);

    assert_matches!(rx.try_recv(), Ok(AppEvent::Logout));
}

#[tokio::test]
async fn slash_copy_state_tracks_turn_complete_final_reply() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: Some("Final reply **markdown**".to_string()),
            completed_at: None,
            duration_ms: None,
        }),
    });

    assert_eq!(
        chat.last_agent_markdown_text(),
        Some("Final reply **markdown**")
    );
}

#[tokio::test]
async fn slash_copy_state_tracks_plan_item_completion() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let plan_text = "## Plan\n\n1. Build it\n2. Test it".to_string();

    chat.handle_codex_event(Event {
        id: "item-plan".into(),
        msg: EventMsg::ItemCompleted(ItemCompletedEvent {
            thread_id: ThreadId::new(),
            turn_id: "turn-1".to_string(),
            item: TurnItem::Plan(PlanItem {
                id: "plan-1".to_string(),
                text: plan_text.clone(),
            }),
        }),
    });
    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: None,
        }),
    });

    assert_eq!(chat.last_agent_markdown_text(), Some(plan_text.as_str()));
}

#[tokio::test]
async fn slash_copy_reports_when_no_copyable_output_exists() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Copy);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert_chatwidget_snapshot!("slash_copy_no_output_info_message", rendered);
    assert!(
        rendered.contains("当前没有可复制的 Agent 回复。"),
        "expected no-output message, got {rendered:?}"
    );
}

#[tokio::test]
async fn zteam_slash_command_opens_workbench_view() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.dispatch_command(SlashCommand::Zteam);

    let height = chat.desired_height(/*width*/ 100);
    let mut terminal = Terminal::new(TestBackend::new(100, height)).expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw zteam workbench");
    assert_chatwidget_snapshot!(
        "zteam_workbench_empty_view",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn zteam_entry_reports_disabled_configuration() {
    let mut config = test_config().await;
    config.zteam_enabled = false;
    let (mut chat, mut rx, _op_rx) =
        make_chatwidget_manual_with_config(config, /*model_override*/ None).await;

    chat.open_zteam_entry();

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert_chatwidget_snapshot!("zteam_entry_disabled_notice", rendered);
    assert!(
        rendered.contains("ZTeam 已在当前 TUI 配置中关闭"),
        "expected disabled zteam notice, got {rendered:?}"
    );
}

fn zteam_test_thread(
    thread_id: ThreadId,
    slot: crate::zteam::WorkerSlot,
) -> codex_app_server_protocol::Thread {
    use codex_app_server_protocol::SessionSource;
    use codex_app_server_protocol::Thread;
    use codex_app_server_protocol::ThreadStatus;
    use codex_protocol::protocol::SubAgentSource;
    use codex_utils_absolute_path::test_support::PathBufExt;
    use codex_utils_absolute_path::test_support::test_path_buf;

    Thread {
        id: thread_id.to_string(),
        forked_from_id: None,
        preview: String::new(),
        ephemeral: false,
        model_provider: "openai".to_string(),
        created_at: 0,
        updated_at: 0,
        status: ThreadStatus::Idle,
        path: None,
        cwd: test_path_buf("/tmp").abs(),
        cli_version: "0.0.0".to_string(),
        source: SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id: ThreadId::from_string("00000000-0000-0000-0000-000000000001")
                .expect("valid thread id"),
            depth: 1,
            parent_model: None,
            agent_path: None,
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

#[tokio::test]
async fn zteam_workbench_updates_with_worker_activity() {
    use codex_app_server_protocol::ItemCompletedNotification;
    use codex_app_server_protocol::ServerNotification;
    use codex_app_server_protocol::ThreadStartedNotification;
    use codex_protocol::models::MessagePhase;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let frontend_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
    let ios_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000015").expect("valid thread");
    let backend_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Zteam);
    chat.mark_zteam_start_requested();
    chat.observe_zteam_thread_notification(
        frontend_id,
        &ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: zteam_test_thread(frontend_id, crate::zteam::WorkerSlot::Frontend),
        }),
    );
    chat.observe_zteam_thread_notification(
        ios_id,
        &ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: zteam_test_thread(ios_id, crate::zteam::WorkerSlot::Ios),
        }),
    );
    chat.observe_zteam_thread_notification(
        backend_id,
        &ServerNotification::ThreadStarted(ThreadStartedNotification {
            thread: zteam_test_thread(backend_id, crate::zteam::WorkerSlot::Backend),
        }),
    );
    chat.record_zteam_dispatch(crate::zteam::WorkerSlot::Frontend, "修复导航栏布局");
    chat.record_zteam_dispatch(crate::zteam::WorkerSlot::Ios, "修复 iOS 列表滚动卡顿");
    chat.record_zteam_relay(
        crate::zteam::WorkerSlot::Frontend,
        crate::zteam::WorkerSlot::Backend,
        "对齐接口字段",
    );
    chat.observe_zteam_thread_notification(
        frontend_id,
        &ServerNotification::ItemCompleted(ItemCompletedNotification {
            item: codex_app_server_protocol::ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "前端阶段结果：工作台布局已完成，并补上移动端断点。".to_string(),
                phase: Some(MessagePhase::FinalAnswer),
                memory_citation: None,
            },
            thread_id: frontend_id.to_string(),
            turn_id: "turn-1".to_string(),
        }),
    );
    chat.observe_zteam_thread_notification(
        ios_id,
        &ServerNotification::ItemCompleted(ItemCompletedNotification {
            item: codex_app_server_protocol::ThreadItem::AgentMessage {
                id: "msg-ios".to_string(),
                text: "iOS 阶段结果：列表滚动卡顿已修复，并统一了安全区间距。".to_string(),
                phase: Some(MessagePhase::FinalAnswer),
                memory_citation: None,
            },
            thread_id: ios_id.to_string(),
            turn_id: "turn-ios".to_string(),
        }),
    );
    chat.observe_zteam_thread_notification(
        backend_id,
        &ServerNotification::ItemCompleted(ItemCompletedNotification {
            item: codex_app_server_protocol::ThreadItem::AgentMessage {
                id: "msg-2".to_string(),
                text: "后端阶段结果：接口字段已统一，并补齐错误态返回。".to_string(),
                phase: Some(MessagePhase::FinalAnswer),
                memory_citation: None,
            },
            thread_id: backend_id.to_string(),
            turn_id: "turn-2".to_string(),
        }),
    );

    let height = chat.desired_height(/*width*/ 100);
    let mut terminal = Terminal::new(TestBackend::new(100, height)).expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw active zteam workbench");
    assert_chatwidget_snapshot!(
        "zteam_workbench_active_view",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn zteam_start_inline_command_requests_app_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Zteam, "start".to_string(), Vec::new());

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::ZteamCommand(crate::zteam::Command::Start))
    );
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn zteam_attach_inline_command_requests_app_event() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Zteam, "attach".to_string(), Vec::new());

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::ZteamCommand(crate::zteam::Command::Attach))
    );
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn zteam_status_inline_command_requests_app_event_while_task_running() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.bottom_pane.set_task_running(/*running*/ true);

    chat.dispatch_command_with_args(SlashCommand::Zteam, "status".to_string(), Vec::new());

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::ZteamCommand(crate::zteam::Command::Status))
    );
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn zteam_frontend_inline_command_is_disabled_while_task_running() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.bottom_pane.set_task_running(/*running*/ true);

    chat.dispatch_command_with_args(
        SlashCommand::Zteam,
        "frontend 修复导航栏布局".to_string(),
        Vec::new(),
    );

    let event = rx.try_recv().expect("expected disabled command error");
    match event {
        AppEvent::InsertHistoryCell(cell) => {
            let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 80));
            assert!(
                rendered.contains("'/zteam' 在任务进行中被禁用。"),
                "expected /zteam task-running error, got {rendered:?}"
            );
        }
        other => panic!("expected InsertHistoryCell error, got {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "expected no follow-up events");
}

#[tokio::test]
async fn zteam_frontend_inline_command_requests_task_dispatch() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(
        SlashCommand::Zteam,
        "frontend 修复导航栏布局".to_string(),
        Vec::new(),
    );

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::ZteamCommand(crate::zteam::Command::Dispatch { worker, message }))
            if worker == crate::zteam::WorkerSlot::Frontend && message == "修复导航栏布局"
    );
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn zteam_ios_inline_command_requests_task_dispatch() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(
        SlashCommand::Zteam,
        "ios 修复列表滚动卡顿".to_string(),
        Vec::new(),
    );

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::ZteamCommand(crate::zteam::Command::Dispatch { worker, message }))
            if worker == crate::zteam::WorkerSlot::Ios && message == "修复列表滚动卡顿"
    );
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn zteam_relay_inline_command_requests_worker_message() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(
        SlashCommand::Zteam,
        "relay frontend backend 对齐接口字段".to_string(),
        Vec::new(),
    );

    assert_matches!(
        rx.try_recv(),
        Ok(AppEvent::ZteamCommand(crate::zteam::Command::Relay { from, to, message }))
            if from == crate::zteam::WorkerSlot::Frontend
                && to == crate::zteam::WorkerSlot::Backend
                && message == "对齐接口字段"
    );
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn zteam_inline_command_rejects_invalid_subcommand() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Zteam, "frontend".to_string(), Vec::new());

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one error message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("用法：/zteam start"),
        "expected zteam usage error, got {rendered:?}"
    );
}

#[tokio::test]
async fn zteam_workbench_shows_reattach_required_workers_and_adapter_summary() {
    use codex_app_server_protocol::FederationThreadStartParams;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    let frontend_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000010").expect("valid thread");
    let ios_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000015").expect("valid thread");
    let backend_id =
        ThreadId::from_string("00000000-0000-0000-0000-000000000020").expect("valid thread");
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Zteam);
    chat.configure_zteam_federation_adapter(Some(FederationThreadStartParams {
        instance_id: None,
        name: "zteam".to_string(),
        role: Some("worker".to_string()),
        scope: Some("workspace".to_string()),
        state_root: Some("/tmp/federation".to_string()),
        lease_ttl_secs: Some(30),
    }));
    chat.restore_zteam_worker(crate::zteam::RecoveredWorker {
        slot: crate::zteam::WorkerSlot::Frontend,
        connection: crate::zteam::WorkerConnection::ReattachRequired(frontend_id),
        source: crate::zteam::WorkerSource::LocalThreadSpawn,
        last_dispatched_task: Some("修复导航栏布局".to_string()),
        last_result: Some("前端阶段结果：等待重新附着。".to_string()),
    });
    chat.restore_zteam_worker(crate::zteam::RecoveredWorker {
        slot: crate::zteam::WorkerSlot::Ios,
        connection: crate::zteam::WorkerConnection::ReattachRequired(ios_id),
        source: crate::zteam::WorkerSource::LocalThreadSpawn,
        last_dispatched_task: Some("修复 iOS 列表滚动卡顿".to_string()),
        last_result: Some("iOS 阶段结果：等待重新附着。".to_string()),
    });
    chat.restore_zteam_worker(crate::zteam::RecoveredWorker {
        slot: crate::zteam::WorkerSlot::Backend,
        connection: crate::zteam::WorkerConnection::ReattachRequired(backend_id),
        source: crate::zteam::WorkerSource::LocalThreadSpawn,
        last_dispatched_task: Some("对齐接口字段".to_string()),
        last_result: Some("后端阶段结果：等待重新附着。".to_string()),
    });

    let height = chat.desired_height(/*width*/ 100);
    let mut terminal = Terminal::new(TestBackend::new(100, height)).expect("create terminal");
    terminal
        .draw(|f| chat.render(f.area(), f.buffer_mut()))
        .expect("draw reattach zteam workbench");
    assert_chatwidget_snapshot!(
        "zteam_workbench_reattach_required_view",
        normalized_backend_snapshot(terminal.backend())
    );
}

#[tokio::test]
async fn paste_image_error_message_is_localized() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    let err = crate::clipboard_paste::PasteImageError::ClipboardUnavailable(
        "Unknown error while interacting with the clipboard".to_string(),
    );
    chat.add_error_message(format!("粘贴图片失败：{err}"));

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one error message");
    let rendered = lines_to_single_string(&cells[0]);
    assert_chatwidget_snapshot!("paste_image_error_message_is_localized", rendered);
    assert!(
        rendered.contains("粘贴图片失败：剪贴板不可用："),
        "expected localized paste error message, got {rendered:?}"
    );
}

#[tokio::test]
async fn slash_copy_state_is_preserved_during_running_task() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: Some("Previous completed reply".to_string()),
            completed_at: None,
            duration_ms: None,
        }),
    });
    chat.on_task_started();

    assert_eq!(
        chat.last_agent_markdown_text(),
        Some("Previous completed reply")
    );
}

#[tokio::test]
async fn slash_copy_state_clears_on_thread_rollback() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: Some("Reply that will be rolled back".to_string()),
            completed_at: None,
            duration_ms: None,
        }),
    });
    chat.handle_codex_event(Event {
        id: "rollback-1".into(),
        msg: EventMsg::ThreadRolledBack(ThreadRolledBackEvent { num_turns: 1 }),
    });

    assert_eq!(chat.last_agent_markdown_text(), None);
}

#[tokio::test]
async fn slash_copy_is_unavailable_when_legacy_agent_message_is_not_repeated_on_turn_complete() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event_replay(Event {
        id: "turn-1".into(),
        msg: EventMsg::AgentMessage(AgentMessageEvent {
            message: "Legacy final message".into(),
            phase: None,
            memory_citation: None,
        }),
    });
    let _ = drain_insert_history(&mut rx);
    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: None,
        }),
    });
    let _ = drain_insert_history(&mut rx);

    chat.dispatch_command(SlashCommand::Copy);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("当前没有可复制的 Agent 回复。"),
        "expected unavailable message, got {rendered:?}"
    );
}
#[tokio::test]
async fn slash_buddy_show_requests_persistent_visibility_update() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Buddy, "show".to_string(), Vec::new());

    assert_matches!(rx.try_recv(), Ok(AppEvent::PersistBuddyVisibility(true)));
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn slash_buddy_full_requests_persistent_full_visibility_update() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Buddy, "full".to_string(), Vec::new());

    assert_matches!(rx.try_recv(), Ok(AppEvent::PersistBuddyFullVisibility));
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn slash_buddy_hide_requests_persistent_visibility_update() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Buddy, "hide".to_string(), Vec::new());

    assert_matches!(rx.try_recv(), Ok(AppEvent::PersistBuddyVisibility(false)));
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn slash_buddy_pet_requires_show_when_hidden() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let _ = chat.bottom_pane.hide_buddy();
    chat.config.tui_show_buddy = false;

    chat.dispatch_command_with_args(SlashCommand::Buddy, "pet".to_string(), Vec::new());

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert_chatwidget_snapshot!("slash_buddy_pet_requires_show_when_hidden", rendered);
    assert!(
        rendered.contains("小伙伴现在藏起来了。"),
        "unexpected hidden buddy message: {rendered:?}"
    );
    assert!(
        rendered.contains("先用 `/buddy show` 让它回来，或用 `/buddy full` 进入全形象常驻。"),
        "unexpected hidden buddy hint: {rendered:?}"
    );
}

#[tokio::test]
async fn slash_buddy_status_reports_traits() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Buddy, "status".to_string(), Vec::new());

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one status info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("小伙伴状态："),
        "unexpected status message: {rendered:?}"
    );
    assert!(
        rendered.contains("峰值属性："),
        "expected trait details in status message: {rendered:?}"
    );
}

#[tokio::test]
async fn slash_buddy_rejects_unknown_subcommand() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command_with_args(SlashCommand::Buddy, "zoom".to_string(), Vec::new());

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one error message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("用法：/buddy [show|full|pet|hide|status]"),
        "unexpected error message: {rendered:?}"
    );
}

#[tokio::test]
async fn slash_copy_uses_agent_message_item_when_turn_complete_omits_final_text() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    complete_assistant_message(
        &mut chat,
        "msg-1",
        "Legacy item final message",
        /*phase*/ None,
    );
    let _ = drain_insert_history(&mut rx);
    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: None,
        }),
    });
    let _ = drain_insert_history(&mut rx);

    chat.dispatch_command(SlashCommand::Copy);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        !rendered.contains("`/copy` 在首次 Codex 输出之前或回滚后不可用。"),
        "expected copy state to be available, got {rendered:?}"
    );
    assert_eq!(
        chat.last_agent_markdown_text(),
        Some("Legacy item final message")
    );
}

#[tokio::test]
async fn slash_copy_does_not_return_stale_output_after_thread_rollback() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });
    complete_assistant_message(
        &mut chat,
        "msg-1",
        "Reply that will be rolled back",
        /*phase*/ None,
    );
    let _ = drain_insert_history(&mut rx);
    chat.handle_codex_event(Event {
        id: "turn-1".into(),
        msg: EventMsg::TurnComplete(TurnCompleteEvent {
            turn_id: "turn-1".to_string(),
            last_agent_message: None,
            completed_at: None,
            duration_ms: None,
        }),
    });
    let _ = drain_insert_history(&mut rx);

    chat.handle_codex_event(Event {
        id: "rollback-1".into(),
        msg: EventMsg::ThreadRolledBack(ThreadRolledBackEvent { num_turns: 1 }),
    });
    let _ = drain_insert_history(&mut rx);

    chat.dispatch_command(SlashCommand::Copy);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected one info message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("当前没有可复制的 Agent 回复。"),
        "expected rollback-cleared copy state message, got {rendered:?}"
    );
}

#[tokio::test]
async fn slash_exit_requests_exit() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Exit);

    assert_matches!(rx.try_recv(), Ok(AppEvent::Exit(ExitMode::ShutdownFirst)));
}

#[tokio::test]
async fn slash_stop_submits_background_terminal_cleanup() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Stop);

    assert_matches!(op_rx.try_recv(), Ok(Op::CleanBackgroundTerminals));
    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected cleanup confirmation message");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("正在停止所有后台终端。"),
        "expected cleanup confirmation, got {rendered:?}"
    );
}

#[tokio::test]
async fn slash_clear_requests_ui_clear_when_idle() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Clear);

    assert_matches!(rx.try_recv(), Ok(AppEvent::ClearUi));
}

#[tokio::test]
async fn slash_clear_is_disabled_while_task_running() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.bottom_pane.set_task_running(/*running*/ true);

    chat.dispatch_command(SlashCommand::Clear);

    let event = rx.try_recv().expect("expected disabled command error");
    match event {
        AppEvent::InsertHistoryCell(cell) => {
            let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 80));
            assert!(
                rendered.contains("'/clear' 在任务进行中被禁用。"),
                "expected /clear task-running error, got {rendered:?}"
            );
        }
        other => panic!("expected InsertHistoryCell error, got {other:?}"),
    }
    assert!(rx.try_recv().is_err(), "expected no follow-up events");
}

#[tokio::test]
async fn slash_memory_drop_reports_stubbed_feature() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::MemoryDrop);

    let event = rx.try_recv().expect("expected unsupported-feature error");
    match event {
        AppEvent::InsertHistoryCell(cell) => {
            let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 80));
            assert!(rendered.contains("记忆维护: TUI 暂未提供。"));
        }
        other => panic!("expected InsertHistoryCell error, got {other:?}"),
    }
    assert!(
        op_rx.try_recv().is_err(),
        "expected no memory op to be sent"
    );
}

#[tokio::test]
async fn slash_mcp_requests_inventory_via_app_server() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Mcp);

    assert!(active_blob(&chat).contains("正在加载 MCP 清单"));
    assert_matches!(rx.try_recv(), Ok(AppEvent::FetchMcpInventory));
    assert!(op_rx.try_recv().is_err(), "expected no core op to be sent");
}

#[tokio::test]
async fn slash_memories_opens_memory_menu() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.set_feature_enabled(Feature::MemoryTool, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Memories);

    let popup = render_bottom_popup(&chat, /*width*/ 80);
    assert!(normalize_rendered_text(&popup).contains("使用记忆"));
    assert_matches!(rx.try_recv(), Err(TryRecvError::Empty));
    assert!(op_rx.try_recv().is_err(), "expected no core op to be sent");
}

#[tokio::test]
async fn slash_memory_update_reports_stubbed_feature() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::MemoryUpdate);

    let event = rx.try_recv().expect("expected unsupported-feature error");
    match event {
        AppEvent::InsertHistoryCell(cell) => {
            let rendered = lines_to_single_string(&cell.display_lines(/*width*/ 80));
            assert!(rendered.contains("记忆维护: TUI 暂未提供。"));
        }
        other => panic!("expected InsertHistoryCell error, got {other:?}"),
    }
    assert!(
        op_rx.try_recv().is_err(),
        "expected no memory op to be sent"
    );
}

#[tokio::test]
async fn slash_resume_opens_picker() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Resume);

    assert_matches!(rx.try_recv(), Ok(AppEvent::OpenResumePicker));
}

#[tokio::test]
async fn slash_fork_requests_current_fork() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Fork);

    assert_matches!(rx.try_recv(), Ok(AppEvent::ForkCurrentSession));
}

#[tokio::test]
async fn slash_rollout_displays_current_path() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let rollout_path = PathBuf::from("/tmp/codex-test-rollout.jsonl");
    chat.current_rollout_path = Some(rollout_path.clone());

    chat.dispatch_command(SlashCommand::Rollout);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected info message for rollout path");
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains(&rollout_path.display().to_string()),
        "expected rollout path to be shown: {rendered}"
    );
}

#[tokio::test]
async fn slash_rollout_handles_missing_path() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Rollout);

    let cells = drain_insert_history(&mut rx);
    assert_eq!(
        cells.len(),
        1,
        "expected info message explaining missing path"
    );
    let rendered = lines_to_single_string(&cells[0]);
    assert!(
        rendered.contains("暂不可用"),
        "expected missing rollout path message: {rendered}"
    );
}

#[tokio::test]
async fn undo_success_events_render_info_messages() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-1".to_string(),
        msg: EventMsg::UndoStarted(UndoStartedEvent {
            message: Some("Undo requested for the last turn...".to_string()),
        }),
    });
    assert!(
        chat.bottom_pane.status_indicator_visible(),
        "status indicator should be visible during undo"
    );

    chat.handle_codex_event(Event {
        id: "turn-1".to_string(),
        msg: EventMsg::UndoCompleted(UndoCompletedEvent {
            success: true,
            message: None,
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected final status only");
    assert!(
        !chat.bottom_pane.status_indicator_visible(),
        "status indicator should be hidden after successful undo"
    );

    let completed = lines_to_single_string(&cells[0]);
    assert!(
        completed.contains("撤销已成功完成。"),
        "expected default success message, got {completed:?}"
    );
}

#[tokio::test]
async fn undo_failure_events_render_error_message() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-2".to_string(),
        msg: EventMsg::UndoStarted(UndoStartedEvent { message: None }),
    });
    assert!(
        chat.bottom_pane.status_indicator_visible(),
        "status indicator should be visible during undo"
    );

    chat.handle_codex_event(Event {
        id: "turn-2".to_string(),
        msg: EventMsg::UndoCompleted(UndoCompletedEvent {
            success: false,
            message: Some("Failed to restore workspace state.".to_string()),
        }),
    });

    let cells = drain_insert_history(&mut rx);
    assert_eq!(cells.len(), 1, "expected final status only");
    assert!(
        !chat.bottom_pane.status_indicator_visible(),
        "status indicator should be hidden after failed undo"
    );

    let completed = lines_to_single_string(&cells[0]);
    assert!(
        completed.contains("Failed to restore workspace state."),
        "expected failure message, got {completed:?}"
    );
}

#[tokio::test]
async fn undo_started_hides_interrupt_hint() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.handle_codex_event(Event {
        id: "turn-hint".to_string(),
        msg: EventMsg::UndoStarted(UndoStartedEvent { message: None }),
    });

    let status = chat
        .bottom_pane
        .status_widget()
        .expect("status indicator should be active");
    assert!(
        !status.interrupt_hint_visible(),
        "undo should hide the interrupt hint because the operation cannot be cancelled"
    );
}

#[tokio::test]
async fn fast_slash_command_updates_and_persists_local_service_tier() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5.3-codex")).await;
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Fast);

    let events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::CodexOp(Op::OverrideTurnContext {
                service_tier: Some(Some(ServiceTier::Fast)),
                ..
            })
        )),
        "expected fast-mode override app event; events: {events:?}"
    );
    assert!(
        events.iter().any(|event| matches!(
            event,
            AppEvent::PersistServiceTierSelection {
                service_tier: Some(ServiceTier::Fast),
            }
        )),
        "expected fast-mode persistence app event; events: {events:?}"
    );

    assert_matches!(op_rx.try_recv(), Err(TryRecvError::Empty));
}

#[tokio::test]
async fn user_turn_carries_service_tier_after_fast_toggle() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5.3-codex")).await;
    chat.thread_id = Some(ThreadId::new());
    set_chatgpt_auth(&mut chat);
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Fast);

    let _events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();

    chat.bottom_pane
        .set_composer_text("hello".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            service_tier: Some(Some(ServiceTier::Fast)),
            ..
        } => {}
        other => panic!("expected Op::UserTurn with fast service tier, got {other:?}"),
    }
}

#[tokio::test]
async fn user_turn_clears_service_tier_after_fast_is_turned_off() {
    let (mut chat, mut rx, mut op_rx) = make_chatwidget_manual(Some("gpt-5.3-codex")).await;
    chat.thread_id = Some(ThreadId::new());
    set_chatgpt_auth(&mut chat);
    chat.set_feature_enabled(Feature::FastMode, /*enabled*/ true);

    chat.dispatch_command(SlashCommand::Fast);
    let _events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();

    chat.dispatch_command_with_args(SlashCommand::Fast, "off".to_string(), Vec::new());
    let _events = std::iter::from_fn(|| rx.try_recv().ok()).collect::<Vec<_>>();

    chat.bottom_pane
        .set_composer_text("hello".to_string(), Vec::new(), Vec::new());
    chat.handle_key_event(KeyEvent::from(KeyCode::Enter));

    match next_submit_op(&mut op_rx) {
        Op::UserTurn {
            service_tier: Some(None),
            ..
        } => {}
        other => panic!("expected Op::UserTurn to clear service tier, got {other:?}"),
    }
}

#[tokio::test]
async fn compact_queues_user_messages_snapshot() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.thread_id = Some(ThreadId::new());
    chat.handle_codex_event(Event {
        id: "turn-start".into(),
        msg: EventMsg::TurnStarted(TurnStartedEvent {
            turn_id: "turn-1".to_string(),
            started_at: None,
            model_context_window: None,
            collaboration_mode_kind: ModeKind::Default,
        }),
    });

    chat.submit_user_message(UserMessage::from(
        "Steer submitted while /compact was running.".to_string(),
    ));
    chat.handle_codex_event(Event {
        id: "steer-rejected".into(),
        msg: EventMsg::Error(ErrorEvent {
            message: "无法在压缩轮次中继续追加".to_string(),
            codex_error_info: Some(CodexErrorInfo::ActiveTurnNotSteerable {
                turn_kind: NonSteerableTurnKind::Compact,
            }),
        }),
    });

    let width: u16 = 80;
    let height: u16 = 18;
    let backend = VT100Backend::new(width, height);
    let mut term = crate::custom_terminal::Terminal::with_options(backend).expect("terminal");
    let desired_height = chat.desired_height(width).min(height);
    term.set_viewport_area(Rect::new(0, height - desired_height, width, desired_height));
    term.draw(|f| {
        chat.render(f.area(), f.buffer_mut());
    })
    .unwrap();
    assert_chatwidget_snapshot!(
        "compact_queues_user_messages_snapshot",
        normalize_snapshot_paths(term.backend().vt100().screen().contents())
    );
}

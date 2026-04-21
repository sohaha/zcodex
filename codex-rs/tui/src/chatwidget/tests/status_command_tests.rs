use super::*;
use crate::app_event::RateLimitRefreshOrigin;
use assert_matches::assert_matches;

fn recv_non_branch_update(rx: &mut tokio::sync::mpsc::UnboundedReceiver<AppEvent>) -> AppEvent {
    loop {
        match rx.try_recv() {
            Ok(AppEvent::StatusLineBranchUpdated { .. }) => continue,
            Ok(event) => return event,
            Err(err) => panic!("expected app event, got {err:?}"),
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_command_renders_immediately_and_refreshes_rate_limits_for_chatgpt_auth() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match recv_non_branch_update(&mut rx) {
        AppEvent::InsertHistoryCell(cell) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected status output before refresh request, got {other:?}"),
    };
    assert!(
        rendered.contains("正在刷新限额"),
        "expected /status to explain the background refresh, got: {rendered}"
    );
    let request_id = match recv_non_branch_update(&mut rx) {
        AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        } => request_id,
        other => panic!("expected rate-limit refresh request, got {other:?}"),
    };
    pretty_assertions::assert_eq!(request_id, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_command_updates_rendered_cell_after_rate_limit_refresh() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);

    chat.dispatch_command(SlashCommand::Status);

    let cell = match recv_non_branch_update(&mut rx) {
        AppEvent::InsertHistoryCell(cell) => cell,
        other => panic!("expected status output before refresh request, got {other:?}"),
    };
    let first_request_id = match recv_non_branch_update(&mut rx) {
        AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        } => request_id,
        other => panic!("expected rate-limit refresh request, got {other:?}"),
    };

    let initial = lines_to_single_string(&cell.display_lines(/*width*/ 80));
    assert!(
        initial.contains("正在刷新限额"),
        "expected initial /status output to show refresh notice, got: {initial}"
    );

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 92.0)));
    chat.finish_status_rate_limit_refresh(first_request_id);

    let updated = lines_to_single_string(&cell.display_lines(/*width*/ 80));
    assert_ne!(
        initial, updated,
        "expected refreshed /status output to change"
    );
    assert!(
        !updated.contains("正在刷新限额"),
        "expected refresh notice to clear after background update, got: {updated}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_command_renders_immediately_without_rate_limit_refresh() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;

    chat.dispatch_command(SlashCommand::Status);

    assert_matches!(
        recv_non_branch_update(&mut rx),
        AppEvent::InsertHistoryCell(_)
    );
    assert!(
        !std::iter::from_fn(|| match rx.try_recv() {
            Ok(AppEvent::StatusLineBranchUpdated { .. }) => None,
            other => other.ok(),
        })
        .any(|event| matches!(event, AppEvent::RefreshRateLimits { .. })),
        "non-ChatGPT sessions should not request a rate-limit refresh for /status"
    );
}

#[tokio::test]
async fn status_command_uses_catalog_default_reasoning_when_config_empty() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(Some("gpt-5.1-codex-max")).await;
    chat.config.model_reasoning_effort = None;

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match recv_non_branch_update(&mut rx) {
        AppEvent::InsertHistoryCell(cell) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected status output, got {other:?}"),
    };
    assert!(
        rendered.contains("gpt-5.1-codex-max (推理 中, 总结 自动)"),
        "expected /status to render the catalog default reasoning effort, got: {rendered}"
    );
}

#[tokio::test]
async fn status_command_renders_instruction_sources_from_thread_session() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    chat.instruction_source_paths = vec![chat.config.cwd.join("AGENTS.md")];

    chat.dispatch_command(SlashCommand::Status);

    let rendered = match recv_non_branch_update(&mut rx) {
        AppEvent::InsertHistoryCell(cell) => {
            lines_to_single_string(&cell.display_lines(/*width*/ 80))
        }
        other => panic!("expected status output, got {other:?}"),
    };
    assert!(
        rendered.contains("Agents.md"),
        "expected /status to render app-server instruction sources, got: {rendered}"
    );
    assert!(
        !rendered.contains("Agents.md  <none>"),
        "expected /status to avoid stale <none> when app-server provided instruction sources, got: {rendered}"
    );
}

#[tokio::test]
async fn status_command_overlapping_refreshes_update_matching_cells_only() {
    let (mut chat, mut rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    set_chatgpt_auth(&mut chat);

    chat.dispatch_command(SlashCommand::Status);
    let first_cell = match recv_non_branch_update(&mut rx) {
        AppEvent::InsertHistoryCell(cell) => cell,
        other => panic!("expected first status output, got {other:?}"),
    };
    let first_request_id = match recv_non_branch_update(&mut rx) {
        AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        } => request_id,
        other => panic!("expected first refresh request, got {other:?}"),
    };

    chat.dispatch_command(SlashCommand::Status);
    let second_cell = match recv_non_branch_update(&mut rx) {
        AppEvent::InsertHistoryCell(cell) => cell,
        other => panic!("expected second status output, got {other:?}"),
    };
    let second_request_id = match recv_non_branch_update(&mut rx) {
        AppEvent::RefreshRateLimits {
            origin: RateLimitRefreshOrigin::StatusCommand { request_id },
        } => request_id,
        other => panic!("expected second refresh request, got {other:?}"),
    };

    assert_ne!(first_request_id, second_request_id);

    chat.finish_status_rate_limit_refresh(first_request_id);

    let first_after_failure = lines_to_single_string(&first_cell.display_lines(/*width*/ 80));
    let second_still_refreshing = lines_to_single_string(&second_cell.display_lines(/*width*/ 80));
    assert!(
        !first_after_failure.contains("正在刷新限额"),
        "expected first status cell to stop refreshing after its request completed, got: {first_after_failure}"
    );
    assert!(
        second_still_refreshing.contains("正在刷新限额"),
        "expected later status cell to keep refreshing until its own request completes, got: {second_still_refreshing}"
    );

    chat.on_rate_limit_snapshot(Some(snapshot(/*percent*/ 92.0)));
    chat.finish_status_rate_limit_refresh(second_request_id);
    let second_after_success = lines_to_single_string(&second_cell.display_lines(/*width*/ 80));
    assert!(
        !second_after_success.contains("正在刷新限额"),
        "expected second status cell to refresh once its own request completed, got: {second_after_success}"
    );
    assert!(chat.refreshing_status_outputs.is_empty());
}

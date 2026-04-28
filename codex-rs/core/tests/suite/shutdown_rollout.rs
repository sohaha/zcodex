use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::Op;
use core_test_support::responses::start_mock_server;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use tracing_test::traced_test;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[traced_test]
async fn shutdown_does_not_record_rollout_items_after_closing_rollout() -> anyhow::Result<()> {
    let server = start_mock_server().await;
    let test = test_codex().build(&server).await?;

    test.codex.submit(Op::Shutdown).await?;
    wait_for_event(&test.codex, |event| {
        matches!(event, EventMsg::ShutdownComplete)
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    logs_assert(|lines: &[&str]| {
        lines
            .iter()
            .find(|line| line.contains("failed to record rollout items"))
            .map(|line| Err(format!("unexpected rollout persistence error: {line}")))
            .unwrap_or_else(|| Ok(()))
    });

    Ok(())
}

use anyhow::Result;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_sequence;
use core_test_support::responses::sse;
use core_test_support::responses::sse_failed;
use core_test_support::responses::start_mock_server;
use core_test_support::skip_if_no_network;
use core_test_support::test_codex::test_codex;
use tokio::time::Duration;
use tokio::time::Instant;
use tokio::time::sleep;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn server_overloaded_retries_same_turn() -> Result<()> {
    skip_if_no_network!(Ok(()));

    let server = start_mock_server().await;
    let response_mock = mount_sse_sequence(
        &server,
        vec![
            sse_failed(
                "resp-1",
                "server_is_overloaded",
                "Selected model is at capacity. Please try a different model.",
            ),
            sse(vec![ev_response_created("resp-2"), ev_completed("resp-2")]),
        ],
    )
    .await;

    let mut builder = test_codex().with_config(|config| {
        config.model_provider.supports_websockets = false;
        config.model_provider.stream_max_retries = Some(1);
        config.model_provider.request_max_retries = Some(0);
    });
    let test = builder.build(&server).await?;

    test.submit_turn("retry when the selected model is temporarily at capacity")
        .await?;

    let deadline = Instant::now() + Duration::from_secs(5);
    while response_mock.requests().len() < 2 {
        assert!(
            Instant::now() < deadline,
            "expected the overloaded turn to retry before timing out"
        );
        sleep(Duration::from_millis(25)).await;
    }

    assert_eq!(response_mock.requests().len(), 2);

    Ok(())
}

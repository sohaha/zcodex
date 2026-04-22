use std::collections::BTreeMap;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use app_test_support::McpProcess;
use app_test_support::create_mock_responses_server_repeating_assistant;
use app_test_support::to_response;
use app_test_support::write_mock_responses_config_toml;
use codex_app_server_protocol::FederationThreadStartParams;
use codex_app_server_protocol::RequestId;
use codex_app_server_protocol::ThreadStartParams;
use codex_app_server_protocol::ThreadStartResponse;
use codex_federation_client::FederationClient;
use codex_federation_daemon::FederationDaemon;
use codex_federation_protocol::Envelope;
use codex_federation_protocol::EnvelopeId;
use codex_federation_protocol::EnvelopePayload;
use codex_federation_protocol::FederationDaemonCommand;
use codex_federation_protocol::Heartbeat;
use codex_federation_protocol::InstanceCard;
use codex_federation_protocol::InstanceId;
use codex_federation_protocol::Lease;
use pretty_assertions::assert_eq;
use tempfile::TempDir;
use tokio::time::sleep;
use tokio::time::timeout;

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(15);

#[tokio::test]
async fn federation_thread_start_bridges_text_task_and_returns_text_result() -> Result<()> {
    let model_server = create_mock_responses_server_repeating_assistant("federation reply").await;
    let codex_home = TempDir::new()?;
    let federation_state_root = codex_home.path().join("federation-state");
    write_mock_responses_config_toml(
        codex_home.path(),
        &model_server.uri(),
        &BTreeMap::new(),
        32_000,
        None,
        "mock_provider",
        "compact",
    )?;

    let daemon = FederationDaemon::new(&federation_state_root)?;
    let endpoint_path = federation_state_root.join("daemon").join("endpoint");
    let daemon_task = tokio::spawn(async move { daemon.run_until_shutdown().await });
    wait_for_endpoint(&endpoint_path).await;

    let mut mcp = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_TIMEOUT, mcp.initialize()).await??;

    let request_id = mcp
        .send_thread_start_request(ThreadStartParams {
            federation: Some(FederationThreadStartParams {
                instance_id: None,
                name: "worker".to_string(),
                role: Some("executor".to_string()),
                scope: Some("repo".to_string()),
                state_root: Some(federation_state_root.to_string_lossy().to_string()),
                lease_ttl_secs: Some(30),
            }),
            ..Default::default()
        })
        .await?;
    let response = timeout(
        DEFAULT_TIMEOUT,
        mcp.read_stream_until_response_message(RequestId::Integer(request_id)),
    )
    .await??;
    let _: ThreadStartResponse = to_response(response)?;

    let client = FederationClient::new(&federation_state_root)?;
    let worker = wait_for_peer(&client, "worker").await?;
    let sender = register_sender(&client, codex_home.path()).await?;
    let sender_id = sender.instance_id;

    let created_at = unix_now();
    let task_envelope = Envelope {
        envelope_id: EnvelopeId::new(),
        sender: sender_id,
        recipient: worker.instance_id,
        created_at,
        expires_at: created_at + 300,
        payload: EnvelopePayload::TextTask {
            text: "say hi".to_string(),
        },
    };
    let send_response = client
        .send(&FederationDaemonCommand::SendEnvelope {
            envelope: task_envelope.clone(),
        })
        .await?;
    assert!(send_response.ok);
    assert_eq!(
        send_response.ack.expect("send should return ack").state,
        codex_federation_protocol::AckState::Accepted
    );

    let result = wait_for_result(&client, sender_id, task_envelope.envelope_id).await?;
    let EnvelopePayload::TextResult { in_reply_to, text } = result.payload else {
        panic!("expected text_result envelope");
    };
    assert_eq!(in_reply_to, task_envelope.envelope_id);
    assert_eq!(text, "federation reply");

    client.send(&FederationDaemonCommand::Shutdown).await?;
    daemon_task.await??;
    Ok(())
}

async fn register_sender(
    client: &FederationClient,
    cwd_root: &std::path::Path,
) -> Result<InstanceCard> {
    let registered_at = unix_now();
    let card = InstanceCard {
        instance_id: InstanceId::new(),
        display_name: "sender".to_string(),
        role: Some("caller".to_string()),
        task_scope: Some("repo".to_string()),
        cwd: cwd_root.to_path_buf(),
        registered_at,
        lease: Lease::new(registered_at, 30).map_err(anyhow::Error::msg)?,
        heartbeat: Heartbeat::new(1, registered_at),
    };
    let response = client
        .send(&FederationDaemonCommand::RegisterInstance { card })
        .await?;
    response.card.context("register sender returned no card")
}

async fn wait_for_peer(client: &FederationClient, display_name: &str) -> Result<InstanceCard> {
    for _ in 0..100 {
        let response = client
            .send(&FederationDaemonCommand::ListPeers {
                requester: None,
                now: unix_now(),
            })
            .await?;
        if let Some(peer) = response
            .peers
            .unwrap_or_default()
            .into_iter()
            .find(|peer| peer.display_name == display_name)
        {
            return Ok(peer);
        }
        sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("timed out waiting for federation peer {display_name}");
}

async fn wait_for_result(
    client: &FederationClient,
    recipient: InstanceId,
    in_reply_to: EnvelopeId,
) -> Result<Envelope> {
    for _ in 0..200 {
        let response = client
            .send(&FederationDaemonCommand::ReadInbox {
                recipient,
                now: unix_now(),
            })
            .await?;
        if let Some(envelope) = response
            .envelopes
            .unwrap_or_default()
            .into_iter()
            .find(|envelope| {
                matches!(
                    envelope.payload,
                    EnvelopePayload::TextResult { in_reply_to: reply_id, .. } if reply_id == in_reply_to
                )
            })
        {
            return Ok(envelope);
        }
        sleep(Duration::from_millis(50)).await;
    }
    anyhow::bail!("timed out waiting for text result envelope");
}

async fn wait_for_endpoint(endpoint_path: &std::path::Path) {
    for _ in 0..100 {
        if endpoint_path.exists() {
            return;
        }
        sleep(Duration::from_millis(20)).await;
    }
    panic!("endpoint should appear: {}", endpoint_path.display());
}

fn unix_now() -> i64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() as i64,
        Err(err) => panic!("system time should be after unix epoch: {err}"),
    }
}

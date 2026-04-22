use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use codex_app_server_protocol::FederationThreadStartParams;
use codex_core::CodexThread;
use codex_federation_client::FederationClient;
use codex_federation_protocol::AckState;
use codex_federation_protocol::Envelope;
use codex_federation_protocol::EnvelopeAck;
use codex_federation_protocol::EnvelopeId;
use codex_federation_protocol::EnvelopePayload;
use codex_federation_protocol::FederationDaemonCommand;
use codex_federation_protocol::Heartbeat;
use codex_federation_protocol::InstanceCard;
use codex_federation_protocol::InstanceId;
use codex_federation_protocol::Lease;
use codex_protocol::protocol::AgentStatus;
use codex_protocol::protocol::Op;
use codex_protocol::user_input::UserInput;
use tokio::time::Duration;
use tokio::time::Instant;
use tracing::info;
use tracing::warn;

const DEFAULT_LEASE_TTL_SECS: u32 = 30;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const INBOX_POLL_INTERVAL: Duration = Duration::from_millis(250);
const TURN_STATUS_POLL_INTERVAL: Duration = Duration::from_millis(100);
const RESULT_EXPIRES_IN_SECS: u32 = 300;
const FALLBACK_RESULT_TEXT: &str = "completed without assistant message";

pub(crate) async fn start_thread_federation_bridge(
    thread: Arc<CodexThread>,
    codex_home: &Path,
    params: FederationThreadStartParams,
    thread_cwd: &Path,
) -> Result<()> {
    let config = BridgeConfig::from_params(codex_home, params, thread_cwd)?;
    let client = FederationClient::new(config.state_root.clone())?;
    let card = register_instance(&client, &config).await?;
    let instance_id = card.instance_id;
    let display_name = card.display_name.clone();

    info!(
        instance_id = %instance_id,
        display_name,
        cwd = %card.cwd.display(),
        "started federation bridge",
    );

    tokio::spawn(async move {
        run_bridge_loop(thread, client, card).await;
    });

    Ok(())
}

struct BridgeConfig {
    state_root: PathBuf,
    instance_id: InstanceId,
    display_name: String,
    role: Option<String>,
    scope: Option<String>,
    cwd: PathBuf,
    lease_ttl_secs: u32,
}

impl BridgeConfig {
    fn from_params(
        codex_home: &Path,
        params: FederationThreadStartParams,
        thread_cwd: &Path,
    ) -> Result<Self> {
        let state_root = params
            .state_root
            .map(PathBuf::from)
            .unwrap_or_else(|| codex_home.join("federation"));
        let instance_id = match params.instance_id {
            Some(instance_id) => InstanceId::from_string(&instance_id)
                .with_context(|| format!("invalid federation instance id: {instance_id}"))?,
            None => InstanceId::new(),
        };
        let lease_ttl_secs = params.lease_ttl_secs.unwrap_or(DEFAULT_LEASE_TTL_SECS);

        Ok(Self {
            state_root,
            instance_id,
            display_name: params.name,
            role: params.role,
            scope: params.scope,
            cwd: thread_cwd.to_path_buf(),
            lease_ttl_secs,
        })
    }
}

async fn register_instance(
    client: &FederationClient,
    config: &BridgeConfig,
) -> Result<InstanceCard> {
    let registered_at = unix_now();
    let lease = Lease::new(registered_at, config.lease_ttl_secs).map_err(anyhow::Error::msg)?;
    let heartbeat = Heartbeat::new(1, registered_at);
    let card = InstanceCard {
        instance_id: config.instance_id,
        display_name: config.display_name.clone(),
        role: config.role.clone(),
        task_scope: config.scope.clone(),
        cwd: config.cwd.clone(),
        registered_at,
        lease,
        heartbeat,
    };
    let response = client
        .send(&FederationDaemonCommand::RegisterInstance { card })
        .await?;
    if !response.ok {
        bail!(response.message);
    }
    response
        .card
        .context("federation register returned no card")
}

async fn run_bridge_loop(
    thread: Arc<CodexThread>,
    client: FederationClient,
    mut card: InstanceCard,
) {
    let mut next_heartbeat_at = Instant::now() + HEARTBEAT_INTERVAL;

    loop {
        match thread.agent_status().await {
            AgentStatus::Shutdown | AgentStatus::NotFound => break,
            _ => {}
        }

        if Instant::now() >= next_heartbeat_at {
            match heartbeat_instance(&client, &card).await {
                Ok(updated_card) => card = updated_card,
                Err(err) => {
                    warn!(instance_id = %card.instance_id, "federation heartbeat failed: {err}")
                }
            }
            next_heartbeat_at = Instant::now() + HEARTBEAT_INTERVAL;
        }

        if !thread_is_idle(&thread).await {
            tokio::time::sleep(INBOX_POLL_INTERVAL).await;
            continue;
        }

        match poll_inbox(&client, card.instance_id).await {
            Ok(Some(envelope)) => {
                if let Err(err) = process_text_task(&thread, &client, &card, envelope).await {
                    warn!(instance_id = %card.instance_id, "federation task processing failed: {err}");
                }
                continue;
            }
            Ok(None) => {}
            Err(err) => {
                warn!(instance_id = %card.instance_id, "federation inbox poll failed: {err}")
            }
        }

        tokio::time::sleep(INBOX_POLL_INTERVAL).await;
    }
}

async fn heartbeat_instance(
    client: &FederationClient,
    card: &InstanceCard,
) -> Result<InstanceCard> {
    let observed_at = unix_now();
    let heartbeat = Heartbeat::new(card.heartbeat.sequence.saturating_add(1), observed_at);
    let lease = Lease::new(observed_at, card.lease.ttl_secs).map_err(anyhow::Error::msg)?;
    let response = client
        .send(&FederationDaemonCommand::Heartbeat {
            instance_id: card.instance_id,
            lease,
            heartbeat,
        })
        .await?;
    if !response.ok {
        bail!(response.message);
    }
    response
        .card
        .context("federation heartbeat returned no card")
}

async fn thread_is_idle(thread: &CodexThread) -> bool {
    matches!(
        thread.agent_status().await,
        AgentStatus::PendingInit | AgentStatus::Completed(_) | AgentStatus::Errored(_)
    )
}

async fn poll_inbox(client: &FederationClient, recipient: InstanceId) -> Result<Option<Envelope>> {
    let response = client
        .send(&FederationDaemonCommand::ReadInbox {
            recipient,
            now: unix_now(),
        })
        .await?;
    if !response.ok {
        bail!(response.message);
    }

    Ok(response.envelopes.and_then(|envelopes| {
        envelopes
            .into_iter()
            .find(|envelope| matches!(envelope.payload, EnvelopePayload::TextTask { .. }))
    }))
}

async fn process_text_task(
    thread: &Arc<CodexThread>,
    client: &FederationClient,
    card: &InstanceCard,
    envelope: Envelope,
) -> Result<()> {
    let EnvelopePayload::TextTask { text } = &envelope.payload else {
        return Ok(());
    };
    let initial_status = thread.agent_status().await;
    let snapshot = thread.config_snapshot().await;
    thread
        .submit(Op::UserTurn {
            items: vec![UserInput::Text {
                text: text.clone(),
                text_elements: Vec::new(),
            }],
            cwd: snapshot.cwd.to_path_buf(),
            approval_policy: snapshot.approval_policy,
            approvals_reviewer: Some(snapshot.approvals_reviewer),
            sandbox_policy: snapshot.sandbox_policy,
            model: snapshot.model,
            effort: None,
            summary: None,
            service_tier: None,
            final_output_json_schema: None,
            collaboration_mode: None,
            personality: None,
        })
        .await
        .context("submit federation text task to local thread")?;

    match wait_for_federated_turn(thread, initial_status).await {
        Ok(last_agent_message) => {
            let result_text =
                last_agent_message.unwrap_or_else(|| FALLBACK_RESULT_TEXT.to_string());
            send_result_envelope(client, card.instance_id, &envelope, result_text).await?;
            write_ack(
                client,
                EnvelopeAck {
                    envelope_id: envelope.envelope_id,
                    recipient: envelope.recipient,
                    state: AckState::Delivered,
                    updated_at: unix_now(),
                    detail: None,
                },
            )
            .await?;
        }
        Err(err) => {
            write_ack(
                client,
                EnvelopeAck {
                    envelope_id: envelope.envelope_id,
                    recipient: envelope.recipient,
                    state: AckState::Rejected,
                    updated_at: unix_now(),
                    detail: Some(err.to_string()),
                },
            )
            .await?;
        }
    }

    Ok(())
}

async fn send_result_envelope(
    client: &FederationClient,
    sender: InstanceId,
    envelope: &Envelope,
    text: String,
) -> Result<()> {
    let created_at = unix_now();
    let result_envelope = Envelope {
        envelope_id: EnvelopeId::new(),
        sender,
        recipient: envelope.sender,
        created_at,
        expires_at: created_at + i64::from(RESULT_EXPIRES_IN_SECS),
        payload: EnvelopePayload::TextResult {
            in_reply_to: envelope.envelope_id,
            text,
        },
    };
    let response = client
        .send(&FederationDaemonCommand::SendEnvelope {
            envelope: result_envelope,
        })
        .await?;
    if !response.ok {
        bail!(response.message);
    }
    let ack = response
        .ack
        .context("federation result send returned no ack")?;
    if ack.state != AckState::Accepted {
        let detail = ack.detail.unwrap_or_else(|| ack.state.state_message());
        bail!("federation result envelope was not accepted: {}", detail);
    }
    Ok(())
}

async fn write_ack(client: &FederationClient, ack: EnvelopeAck) -> Result<()> {
    let response = client
        .send(&FederationDaemonCommand::WriteAck { ack })
        .await?;
    if !response.ok {
        bail!(response.message);
    }
    Ok(())
}

async fn wait_for_federated_turn(
    thread: &CodexThread,
    initial_status: AgentStatus,
) -> Result<Option<String>> {
    let mut saw_running = false;

    loop {
        match thread.agent_status().await {
            AgentStatus::PendingInit | AgentStatus::Interrupted => {}
            AgentStatus::Running => {
                saw_running = true;
            }
            AgentStatus::Completed(last_agent_message) => {
                if saw_running || !matches!(initial_status, AgentStatus::Completed(_)) {
                    return Ok(last_agent_message);
                }
            }
            AgentStatus::Errored(message) => {
                if saw_running || !matches!(initial_status, AgentStatus::Errored(_)) {
                    bail!("local turn failed: {message}");
                }
            }
            AgentStatus::Shutdown => {
                bail!("local thread shut down while processing federation task");
            }
            AgentStatus::NotFound => {
                bail!("local thread disappeared while processing federation task");
            }
        }

        tokio::time::sleep(TURN_STATUS_POLL_INTERVAL).await;
    }
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_secs() as i64
}

trait AckStateMessage {
    fn state_message(self) -> String;
}

impl AckStateMessage for AckState {
    fn state_message(self) -> String {
        match self {
            AckState::Accepted => "accepted".to_string(),
            AckState::Delivered => "delivered".to_string(),
            AckState::Expired => "expired".to_string(),
            AckState::Rejected => "rejected".to_string(),
        }
    }
}

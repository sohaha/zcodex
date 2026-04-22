use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use codex_federation_protocol::FederationDaemonCommand;
use codex_federation_protocol::FederationDaemonResponse;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
#[cfg(not(unix))]
use tokio::net::TcpListener;
#[cfg(not(unix))]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixListener;
#[cfg(unix)]
use tokio::net::UnixStream;
use tokio::sync::Mutex;
use tokio::sync::Notify;

use crate::store::FederationStore;

pub struct FederationDaemon {
    store: Arc<Mutex<FederationStore>>,
}

impl FederationDaemon {
    pub fn new(state_root: impl Into<PathBuf>) -> Result<Self> {
        Ok(Self {
            store: Arc::new(Mutex::new(FederationStore::new(state_root)?)),
        })
    }

    pub async fn run_until_shutdown(&self) -> Result<()> {
        #[cfg(unix)]
        {
            self.run_unix().await
        }

        #[cfg(not(unix))]
        {
            self.run_tcp().await
        }
    }

    #[cfg(unix)]
    async fn run_unix(&self) -> Result<()> {
        let endpoint_path = self.store.lock().await.layout().daemon_endpoint_path();
        let pid_path = self.store.lock().await.layout().daemon_pid_path();
        prepare_endpoint_parent(&endpoint_path)?;
        remove_file_if_exists(&endpoint_path)?;
        remove_file_if_exists(&pid_path)?;
        let listener = UnixListener::bind(&endpoint_path)
            .with_context(|| format!("bind federation socket {}", endpoint_path.display()))?;
        write_pid_file(&pid_path)?;
        let shutdown = Arc::new(Notify::new());

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let store = Arc::clone(&self.store);
                    let shutdown = Arc::clone(&shutdown);
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, store, shutdown).await;
                    });
                }
                _ = shutdown.notified() => {
                    break;
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    break;
                }
            }
        }

        remove_file_if_exists(&endpoint_path)?;
        remove_file_if_exists(&pid_path)?;
        Ok(())
    }

    #[cfg(not(unix))]
    async fn run_tcp(&self) -> Result<()> {
        let endpoint_path = self.store.lock().await.layout().daemon_endpoint_path();
        let pid_path = self.store.lock().await.layout().daemon_pid_path();
        prepare_endpoint_parent(&endpoint_path)?;
        remove_file_if_exists(&endpoint_path)?;
        remove_file_if_exists(&pid_path)?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        fs::write(&endpoint_path, listener.local_addr()?.to_string())
            .with_context(|| format!("write federation endpoint {}", endpoint_path.display()))?;
        write_pid_file(&pid_path)?;
        let shutdown = Arc::new(Notify::new());

        loop {
            tokio::select! {
                accept_result = listener.accept() => {
                    let (stream, _) = accept_result?;
                    let store = Arc::clone(&self.store);
                    let shutdown = Arc::clone(&shutdown);
                    tokio::spawn(async move {
                        let _ = serve_connection(stream, store, shutdown).await;
                    });
                }
                _ = shutdown.notified() => {
                    break;
                }
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    break;
                }
            }
        }

        remove_file_if_exists(&endpoint_path)?;
        remove_file_if_exists(&pid_path)?;
        Ok(())
    }
}

async fn serve_connection<T>(
    stream: T,
    store: Arc<Mutex<FederationStore>>,
    shutdown: Arc<Notify>,
) -> Result<()>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let (reader, mut writer) = tokio::io::split(stream);
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let command = match serde_json::from_str::<FederationDaemonCommand>(&line) {
            Ok(command) => command,
            Err(err) => {
                let response = FederationDaemonResponse::error(format!("invalid command: {err}"));
                writer
                    .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
                    .await?;
                continue;
            }
        };
        let (response, should_shutdown) = handle_command(&store, command).await;
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        if should_shutdown {
            shutdown.notify_waiters();
            break;
        }
    }

    Ok(())
}

async fn handle_command(
    store: &Arc<Mutex<FederationStore>>,
    command: FederationDaemonCommand,
) -> (FederationDaemonResponse, bool) {
    let should_shutdown = matches!(command, FederationDaemonCommand::Shutdown);
    let outcome: Result<FederationDaemonResponse> = match command {
        FederationDaemonCommand::Ping => Ok(FederationDaemonResponse::ok("pong")),
        FederationDaemonCommand::RegisterInstance { card } => store
            .lock()
            .await
            .register_instance(card)
            .map(|card| FederationDaemonResponse {
                card: Some(card),
                ..FederationDaemonResponse::ok("instance registered")
            }),
        FederationDaemonCommand::Heartbeat {
            instance_id,
            lease,
            heartbeat,
        } => store
            .lock()
            .await
            .heartbeat_instance(instance_id, lease, heartbeat)
            .map(|card| FederationDaemonResponse {
                card: Some(card),
                ..FederationDaemonResponse::ok("heartbeat accepted")
            }),
        FederationDaemonCommand::ListPeers { requester, now } => store
            .lock()
            .await
            .list_peers(requester, now)
            .map(|peers| FederationDaemonResponse {
                peers: Some(peers),
                ..FederationDaemonResponse::ok("peers listed")
            }),
        FederationDaemonCommand::SendEnvelope { envelope } => {
            store.lock().await.send_envelope(envelope).map(|ack| {
                let message = match ack.state {
                    codex_federation_protocol::AckState::Accepted => "envelope accepted",
                    codex_federation_protocol::AckState::Rejected => "envelope rejected",
                    codex_federation_protocol::AckState::Delivered => "envelope delivered",
                    codex_federation_protocol::AckState::Expired => "envelope expired",
                };
                FederationDaemonResponse {
                    ack: Some(ack),
                    ..FederationDaemonResponse::ok(message)
                }
            })
        }
        FederationDaemonCommand::ReadInbox { recipient, now } => store
            .lock()
            .await
            .read_inbox(recipient, now)
            .map(|envelopes| FederationDaemonResponse {
                envelopes: Some(envelopes),
                ..FederationDaemonResponse::ok("inbox read")
            }),
        FederationDaemonCommand::WriteAck { ack } => {
            store
                .lock()
                .await
                .write_ack(ack)
                .map(|ack| FederationDaemonResponse {
                    ack: Some(ack),
                    ..FederationDaemonResponse::ok("ack written")
                })
        }
        FederationDaemonCommand::Cleanup { now } => {
            store
                .lock()
                .await
                .cleanup(now)
                .map(|cleanup| FederationDaemonResponse {
                    cleanup: Some(cleanup),
                    ..FederationDaemonResponse::ok("cleanup complete")
                })
        }
        FederationDaemonCommand::Shutdown => Ok(FederationDaemonResponse::ok("shutdown requested")),
    };

    match outcome {
        Ok(response) => (response, should_shutdown),
        Err(err) => (FederationDaemonResponse::error(err.to_string()), false),
    }
}

fn prepare_endpoint_parent(path: &Path) -> Result<()> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };
    fs::create_dir_all(parent)
        .with_context(|| format!("create federation daemon dir {}", parent.display()))
}

fn write_pid_file(pid_path: &Path) -> Result<()> {
    prepare_endpoint_parent(pid_path)?;
    fs::write(pid_path, std::process::id().to_string())
        .with_context(|| format!("write federation daemon pid {}", pid_path.display()))
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).with_context(|| format!("remove file {}", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use codex_federation_protocol::AckState;
    use codex_federation_protocol::Envelope;
    use codex_federation_protocol::EnvelopeAck;
    use codex_federation_protocol::EnvelopeId;
    use codex_federation_protocol::EnvelopePayload;
    use codex_federation_protocol::FederationDaemonCommand;
    use codex_federation_protocol::FederationDaemonResponse;
    use codex_federation_protocol::Heartbeat;
    use codex_federation_protocol::InstanceCard;
    use codex_federation_protocol::InstanceId;
    use codex_federation_protocol::Lease;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use tokio::io::AsyncBufReadExt;
    use tokio::io::AsyncWriteExt;
    use tokio::io::BufReader;
    #[cfg(not(unix))]
    use tokio::net::TcpStream;
    #[cfg(unix)]
    use tokio::net::UnixStream;

    use super::FederationDaemon;

    #[tokio::test]
    async fn daemon_serves_register_send_read_ack_and_shutdown() {
        let tempdir = TempDir::new().expect("tempdir");
        let daemon = FederationDaemon::new(tempdir.path()).expect("daemon");
        let endpoint_path = tempdir.path().join("daemon").join("endpoint");

        let daemon_task = tokio::spawn(async move { daemon.run_until_shutdown().await });
        wait_for_endpoint(&endpoint_path).await;

        let sender = test_card("sender", "/workspace/sender", 100, 60, 110);
        let recipient = test_card("recipient", "/workspace/recipient", 100, 60, 110);
        let envelope = Envelope {
            envelope_id: EnvelopeId::default(),
            sender: sender.instance_id,
            recipient: recipient.instance_id,
            created_at: 120,
            expires_at: 180,
            payload: EnvelopePayload::TextTask {
                text: "summarize repository".to_string(),
            },
        };

        let register_sender = send_command(
            &endpoint_path,
            FederationDaemonCommand::RegisterInstance {
                card: sender.clone(),
            },
        )
        .await;
        assert!(register_sender.ok);
        let register_recipient = send_command(
            &endpoint_path,
            FederationDaemonCommand::RegisterInstance {
                card: recipient.clone(),
            },
        )
        .await;
        assert!(register_recipient.ok);

        let send_response = send_command(
            &endpoint_path,
            FederationDaemonCommand::SendEnvelope {
                envelope: envelope.clone(),
            },
        )
        .await;
        assert_eq!(send_response.ack.expect("ack").state, AckState::Accepted);

        let inbox_response = send_command(
            &endpoint_path,
            FederationDaemonCommand::ReadInbox {
                recipient: recipient.instance_id,
                now: 121,
            },
        )
        .await;
        assert_eq!(
            inbox_response.envelopes.expect("envelopes"),
            vec![envelope.clone()]
        );

        let write_ack_response = send_command(
            &endpoint_path,
            FederationDaemonCommand::WriteAck {
                ack: EnvelopeAck {
                    envelope_id: envelope.envelope_id,
                    recipient: recipient.instance_id,
                    state: AckState::Delivered,
                    updated_at: 122,
                    detail: None,
                },
            },
        )
        .await;
        assert!(write_ack_response.ok);

        let empty_inbox = send_command(
            &endpoint_path,
            FederationDaemonCommand::ReadInbox {
                recipient: recipient.instance_id,
                now: 123,
            },
        )
        .await;
        assert_eq!(
            empty_inbox.envelopes.expect("envelopes"),
            Vec::<Envelope>::new()
        );

        let shutdown = send_command(&endpoint_path, FederationDaemonCommand::Shutdown).await;
        assert!(shutdown.ok);

        daemon_task
            .await
            .expect("daemon task should join")
            .expect("daemon should exit cleanly");
        assert!(!endpoint_path.exists());
    }

    async fn wait_for_endpoint(endpoint_path: &PathBuf) {
        for _ in 0..50 {
            if endpoint_path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("endpoint should appear: {}", endpoint_path.display());
    }

    async fn send_command(
        endpoint_path: &PathBuf,
        command: FederationDaemonCommand,
    ) -> FederationDaemonResponse {
        #[cfg(unix)]
        let stream = UnixStream::connect(endpoint_path)
            .await
            .expect("unix socket should connect");

        #[cfg(not(unix))]
        let stream = {
            let address = tokio::fs::read_to_string(endpoint_path)
                .await
                .expect("endpoint file should exist");
            TcpStream::connect(address.trim())
                .await
                .expect("tcp endpoint should connect")
        };

        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        writer
            .write_all(
                format!(
                    "{}\n",
                    serde_json::to_string(&command).expect("command json should serialize")
                )
                .as_bytes(),
            )
            .await
            .expect("command should write");
        let line = lines
            .next_line()
            .await
            .expect("response line should read")
            .expect("daemon should return a response");
        serde_json::from_str(&line).expect("response should deserialize")
    }

    fn test_card(
        name: &str,
        cwd: &str,
        issued_at: i64,
        ttl_secs: u32,
        heartbeat_at: i64,
    ) -> InstanceCard {
        InstanceCard {
            instance_id: InstanceId::default(),
            display_name: name.to_string(),
            role: Some("worker".to_string()),
            task_scope: Some("scope".to_string()),
            cwd: PathBuf::from(cwd),
            registered_at: issued_at,
            lease: Lease::new(issued_at, ttl_secs).expect("lease"),
            heartbeat: Heartbeat::new(1, heartbeat_at),
        }
    }
}

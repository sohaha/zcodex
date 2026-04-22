use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use codex_federation_protocol::FederationDaemonCommand;
use codex_federation_protocol::FederationDaemonResponse;
use codex_federation_protocol::FederationStateLayout;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
#[cfg(not(unix))]
use tokio::net::TcpStream;
#[cfg(unix)]
use tokio::net::UnixStream;

pub struct FederationClient {
    layout: FederationStateLayout,
}

impl FederationClient {
    pub fn new(state_root: impl Into<PathBuf>) -> Result<Self> {
        let layout = FederationStateLayout::new(state_root)?;
        Ok(Self { layout })
    }

    pub fn state_root(&self) -> &Path {
        self.layout.root()
    }

    pub fn endpoint_path(&self) -> PathBuf {
        self.layout.daemon_endpoint_path()
    }

    pub async fn send(
        &self,
        command: &FederationDaemonCommand,
    ) -> Result<FederationDaemonResponse> {
        #[cfg(unix)]
        let stream = UnixStream::connect(self.endpoint_path())
            .await
            .with_context(|| {
                format!(
                    "connect federation daemon {}",
                    self.endpoint_path().display()
                )
            })?;

        #[cfg(not(unix))]
        let stream = {
            let endpoint = tokio::fs::read_to_string(self.endpoint_path())
                .await
                .with_context(|| {
                    format!(
                        "read federation daemon endpoint {}",
                        self.endpoint_path().display()
                    )
                })?;
            TcpStream::connect(endpoint.trim())
                .await
                .with_context(|| format!("connect federation daemon {}", endpoint.trim()))?
        };

        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        writer
            .write_all(format!("{}\n", serde_json::to_string(command)?).as_bytes())
            .await?;
        let response = lines
            .next_line()
            .await?
            .context("federation daemon returned no response")?;
        Ok(serde_json::from_str(&response)?)
    }

    pub async fn ping(&self) -> Result<FederationDaemonResponse> {
        self.send(&FederationDaemonCommand::Ping).await
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use codex_federation_daemon::FederationDaemon;
    use codex_federation_protocol::FederationDaemonCommand;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use tokio::time::Duration;

    use super::FederationClient;

    #[tokio::test]
    async fn client_can_ping_daemon() {
        let tempdir = TempDir::new().expect("tempdir");
        let daemon = FederationDaemon::new(tempdir.path()).expect("daemon");
        let endpoint_path = PathBuf::from(tempdir.path())
            .join("daemon")
            .join("endpoint");
        let daemon_task = tokio::spawn(async move { daemon.run_until_shutdown().await });
        wait_for_endpoint(&endpoint_path).await;

        let client = FederationClient::new(tempdir.path()).expect("client");
        let response = client.ping().await.expect("ping");
        assert!(response.ok);
        assert_eq!(response.message, "pong");

        client
            .send(&FederationDaemonCommand::Shutdown)
            .await
            .expect("shutdown");
        daemon_task
            .await
            .expect("daemon join should succeed")
            .expect("daemon should exit cleanly");
    }

    async fn wait_for_endpoint(endpoint_path: &std::path::Path) {
        for _ in 0..50 {
            if endpoint_path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("endpoint should appear: {}", endpoint_path.display());
    }
}

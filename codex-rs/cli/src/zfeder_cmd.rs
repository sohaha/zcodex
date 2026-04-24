use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use codex_core::config::find_codex_home;
use codex_federation_client::FederationClient;
use codex_federation_daemon::FederationDaemon;
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
use codex_utils_cli::CODEX_SELF_EXE_ENV_VAR;
use serde::Serialize;
use tokio::process::Command;

#[derive(Debug, Parser)]
pub struct ZfederCli {
    #[command(subcommand)]
    pub subcommand: ZfederSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum ZfederSubcommand {
    /// 启动、探测或停止本地 zfeder daemon。
    Daemon(ZfederDaemonCli),

    /// 注册一个 federation 实例。
    Register(RegisterArgs),

    /// 列出当前在线 peers。
    Peers(PeersArgs),

    /// 发送一个文本任务信封。
    Send(SendArgs),

    /// 读取指定实例的收件箱。
    Inbox(InboxArgs),

    /// 写入一个 ack。
    Ack(AckArgs),

    /// Internal: run the federation daemon in the foreground.
    #[clap(hide = true, name = "internal-daemon")]
    InternalDaemon(InternalDaemonArgs),
}

#[derive(Debug, Parser)]
pub struct ZfederDaemonCli {
    #[command(subcommand)]
    subcommand: ZfederDaemonSubcommand,
}

#[derive(Debug, Subcommand)]
enum ZfederDaemonSubcommand {
    Start(DaemonControlArgs),
    Ping(DaemonControlArgs),
    Stop(DaemonControlArgs),
}

#[derive(Debug, Parser, Clone)]
struct DaemonControlArgs {
    #[arg(long = "state-root", value_name = "PATH")]
    state_root: Option<PathBuf>,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Parser, Clone)]
pub struct RegisterArgs {
    #[command(flatten)]
    target: ZfederTargetArgs,

    #[arg(long = "instance-id", value_name = "UUID")]
    instance_id: Option<String>,

    #[arg(long = "name", value_name = "NAME")]
    name: String,

    #[arg(long = "role", value_name = "ROLE")]
    role: Option<String>,

    #[arg(long = "scope", value_name = "SCOPE")]
    scope: Option<String>,

    #[arg(long = "cwd", value_name = "PATH")]
    cwd: Option<PathBuf>,

    #[arg(long = "lease-ttl", value_name = "SECS", default_value_t = 30)]
    lease_ttl_secs: u32,

    #[arg(long = "registered-at", value_name = "UNIX_SECS")]
    registered_at: Option<i64>,

    #[arg(long = "heartbeat-seq", value_name = "SEQ", default_value_t = 1)]
    heartbeat_sequence: u64,

    #[arg(long = "heartbeat-at", value_name = "UNIX_SECS")]
    heartbeat_at: Option<i64>,
}

#[derive(Debug, Parser, Clone)]
pub struct PeersArgs {
    #[command(flatten)]
    target: ZfederTargetArgs,

    #[arg(long = "requester", value_name = "UUID")]
    requester: Option<String>,

    #[arg(long = "now", value_name = "UNIX_SECS")]
    now: Option<i64>,
}

#[derive(Debug, Parser, Clone)]
pub struct SendArgs {
    #[command(flatten)]
    target: ZfederTargetArgs,

    #[arg(long = "envelope-id", value_name = "UUID")]
    envelope_id: Option<String>,

    #[arg(long = "sender", value_name = "UUID")]
    sender: String,

    #[arg(long = "recipient", value_name = "UUID")]
    recipient: String,

    #[arg(long = "text", value_name = "TEXT")]
    text: String,

    #[arg(long = "created-at", value_name = "UNIX_SECS")]
    created_at: Option<i64>,

    #[arg(long = "expires-in", value_name = "SECS", default_value_t = 300)]
    expires_in_secs: u32,
}

#[derive(Debug, Parser, Clone)]
pub struct InboxArgs {
    #[command(flatten)]
    target: ZfederTargetArgs,

    #[arg(long = "recipient", value_name = "UUID")]
    recipient: String,

    #[arg(long = "now", value_name = "UNIX_SECS")]
    now: Option<i64>,
}

#[derive(Debug, Parser, Clone)]
pub struct AckArgs {
    #[command(flatten)]
    target: ZfederTargetArgs,

    #[arg(long = "recipient", value_name = "UUID")]
    recipient: String,

    #[arg(long = "envelope-id", value_name = "UUID")]
    envelope_id: String,

    #[arg(long = "state", value_enum)]
    state: AckStateArg,

    #[arg(long = "updated-at", value_name = "UNIX_SECS")]
    updated_at: Option<i64>,

    #[arg(long = "detail", value_name = "TEXT")]
    detail: Option<String>,
}

#[derive(Debug, Parser, Clone)]
pub struct InternalDaemonArgs {
    #[arg(long = "state-root", value_name = "PATH")]
    state_root: Option<PathBuf>,
}

#[derive(Debug, Parser, Clone)]
struct ZfederTargetArgs {
    #[arg(long = "state-root", value_name = "PATH")]
    state_root: Option<PathBuf>,

    #[arg(long)]
    json: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum AckStateArg {
    Accepted,
    Delivered,
    Rejected,
    Expired,
}

impl ZfederCli {
    pub async fn run(self) -> Result<()> {
        match self.subcommand {
            ZfederSubcommand::Daemon(cli) => cli.run().await,
            ZfederSubcommand::Register(args) => run_register_command(args).await,
            ZfederSubcommand::Peers(args) => run_peers_command(args).await,
            ZfederSubcommand::Send(args) => run_send_command(args).await,
            ZfederSubcommand::Inbox(args) => run_inbox_command(args).await,
            ZfederSubcommand::Ack(args) => run_ack_command(args).await,
            ZfederSubcommand::InternalDaemon(args) => run_internal_daemon(args).await,
        }
    }
}

impl ZfederDaemonCli {
    async fn run(self) -> Result<()> {
        match self.subcommand {
            ZfederDaemonSubcommand::Start(args) => run_daemon_start_command(args).await,
            ZfederDaemonSubcommand::Ping(args) => run_daemon_ping_command(args).await,
            ZfederDaemonSubcommand::Stop(args) => run_daemon_stop_command(args).await,
        }
    }
}

async fn run_internal_daemon(args: InternalDaemonArgs) -> Result<()> {
    let state_root = resolve_state_root(args.state_root)?;
    FederationDaemon::new(state_root)?
        .run_until_shutdown()
        .await
}

async fn run_daemon_start_command(args: DaemonControlArgs) -> Result<()> {
    let state_root = resolve_state_root(args.state_root)?;
    let client = FederationClient::new(state_root.clone())?;
    let started = ensure_daemon_running(&client, &launcher_path()?).await?;
    let response = client.ping().await?;
    let payload = serde_json::json!({
        "started": started,
        "stateRoot": state_root,
        "endpoint": client.endpoint_path(),
        "response": response,
    });
    print_output(args.json, &payload, || {
        vec![
            format!("已启动：{started}"),
            format!("状态目录：{}", state_root.display()),
            format!("端点：{}", client.endpoint_path().display()),
            format!(
                "消息：{}",
                payload["response"]["message"].as_str().unwrap_or_default()
            ),
        ]
    })
}

async fn run_daemon_ping_command(args: DaemonControlArgs) -> Result<()> {
    let state_root = resolve_state_root(args.state_root)?;
    let client = FederationClient::new(state_root.clone())?;
    let response = client.ping().await?;
    print_response(
        args.json,
        &serde_json::json!({
            "stateRoot": state_root,
            "endpoint": client.endpoint_path(),
            "response": &response,
        }),
        || {
            vec![
                format!("状态目录：{}", state_root.display()),
                format!("端点：{}", client.endpoint_path().display()),
                format!("消息：{}", response.message),
            ]
        },
    )
}

async fn run_daemon_stop_command(args: DaemonControlArgs) -> Result<()> {
    let state_root = resolve_state_root(args.state_root)?;
    let client = FederationClient::new(state_root.clone())?;
    let payload = match client.send(&FederationDaemonCommand::Shutdown).await {
        Ok(response) => {
            wait_for_endpoint_removal(&client.endpoint_path()).await;
            serde_json::json!({
                "stopped": true,
                "stateRoot": state_root,
                "response": response,
            })
        }
        Err(err) => serde_json::json!({
            "stopped": false,
            "stateRoot": state_root,
            "message": format!("daemon 未运行：{err}"),
        }),
    };
    print_output(args.json, &payload, || {
        vec![
            format!("已停止：{}", payload["stopped"].as_bool().unwrap_or(false)),
            format!("状态目录：{}", state_root.display()),
            format!(
                "消息：{}",
                payload["message"]
                    .as_str()
                    .or_else(|| payload["response"]["message"].as_str())
                    .unwrap_or_default()
            ),
        ]
    })
}

async fn run_register_command(args: RegisterArgs) -> Result<()> {
    let state_root = resolve_state_root(args.target.state_root.clone())?;
    let client = FederationClient::new(state_root.clone())?;
    ensure_daemon_running(&client, &launcher_path()?).await?;
    let response = execute_register(&client, &args).await?;
    print_response(args.target.json, &response, || {
        render_register_response(&state_root, &response)
    })
}

async fn run_peers_command(args: PeersArgs) -> Result<()> {
    let state_root = resolve_state_root(args.target.state_root.clone())?;
    let client = FederationClient::new(state_root.clone())?;
    ensure_daemon_running(&client, &launcher_path()?).await?;
    let response = execute_peers(&client, &args).await?;
    print_response(args.target.json, &response, || {
        render_peers_response(&state_root, &response)
    })
}

async fn run_send_command(args: SendArgs) -> Result<()> {
    let state_root = resolve_state_root(args.target.state_root.clone())?;
    let client = FederationClient::new(state_root.clone())?;
    ensure_daemon_running(&client, &launcher_path()?).await?;
    let response = execute_send(&client, &args).await?;
    print_response(args.target.json, &response, || {
        render_send_response(&state_root, &response)
    })
}

async fn run_inbox_command(args: InboxArgs) -> Result<()> {
    let state_root = resolve_state_root(args.target.state_root.clone())?;
    let client = FederationClient::new(state_root.clone())?;
    ensure_daemon_running(&client, &launcher_path()?).await?;
    let response = execute_inbox(&client, &args).await?;
    print_response(args.target.json, &response, || {
        render_inbox_response(&state_root, &response)
    })
}

async fn run_ack_command(args: AckArgs) -> Result<()> {
    let state_root = resolve_state_root(args.target.state_root.clone())?;
    let client = FederationClient::new(state_root.clone())?;
    ensure_daemon_running(&client, &launcher_path()?).await?;
    let response = execute_ack(&client, &args).await?;
    print_response(args.target.json, &response, || {
        render_ack_response(&state_root, &response)
    })
}

async fn execute_register(
    client: &FederationClient,
    args: &RegisterArgs,
) -> Result<FederationDaemonResponse> {
    let registered_at = args.registered_at.unwrap_or_else(unix_now);
    let heartbeat_at = args.heartbeat_at.unwrap_or(registered_at);
    let card = InstanceCard {
        instance_id: parse_instance_id(args.instance_id.as_deref())?,
        display_name: args.name.clone(),
        role: args.role.clone(),
        task_scope: args.scope.clone(),
        cwd: match &args.cwd {
            Some(cwd) => cwd.clone(),
            None => std::env::current_dir().context("读取当前工作目录失败")?,
        },
        registered_at,
        lease: Lease::new(registered_at, args.lease_ttl_secs).map_err(anyhow::Error::msg)?,
        heartbeat: Heartbeat::new(args.heartbeat_sequence, heartbeat_at),
    };
    send_checked(client, FederationDaemonCommand::RegisterInstance { card }).await
}

async fn execute_peers(
    client: &FederationClient,
    args: &PeersArgs,
) -> Result<FederationDaemonResponse> {
    send_checked(
        client,
        FederationDaemonCommand::ListPeers {
            requester: parse_optional_instance_id(args.requester.as_deref())?,
            now: args.now.unwrap_or_else(unix_now),
        },
    )
    .await
}

async fn execute_send(client: &FederationClient, args: &SendArgs) -> Result<FederationDaemonResponse> {
    let created_at = args.created_at.unwrap_or_else(unix_now);
    let expires_at = created_at
        .checked_add(i64::from(args.expires_in_secs))
        .context("计算 expires_at 失败")?;
    let envelope = Envelope {
        envelope_id: parse_envelope_id(args.envelope_id.as_deref())?,
        sender: parse_required_instance_id(&args.sender)?,
        recipient: parse_required_instance_id(&args.recipient)?,
        created_at,
        expires_at,
        payload: EnvelopePayload::TextTask {
            text: args.text.clone(),
        },
    };
    send_checked(client, FederationDaemonCommand::SendEnvelope { envelope }).await
}

async fn execute_inbox(
    client: &FederationClient,
    args: &InboxArgs,
) -> Result<FederationDaemonResponse> {
    send_checked(
        client,
        FederationDaemonCommand::ReadInbox {
            recipient: parse_required_instance_id(&args.recipient)?,
            now: args.now.unwrap_or_else(unix_now),
        },
    )
    .await
}

async fn execute_ack(client: &FederationClient, args: &AckArgs) -> Result<FederationDaemonResponse> {
    send_checked(
        client,
        FederationDaemonCommand::WriteAck {
            ack: EnvelopeAck {
                envelope_id: parse_required_envelope_id(&args.envelope_id)?,
                recipient: parse_required_instance_id(&args.recipient)?,
                state: args.state.into(),
                updated_at: args.updated_at.unwrap_or_else(unix_now),
                detail: args.detail.clone(),
            },
        },
    )
    .await
}

async fn send_checked(
    client: &FederationClient,
    command: FederationDaemonCommand,
) -> Result<FederationDaemonResponse> {
    let response = client.send(&command).await?;
    if response.ok {
        Ok(response)
    } else {
        Err(anyhow::Error::msg(response.message))
    }
}

fn resolve_state_root(state_root: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(state_root) = state_root {
        return Ok(state_root);
    }
    Ok(find_codex_home()
        .context("解析 CODEX_HOME 失败")?
        .join("federation")
        .to_path_buf())
}

fn launcher_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(CODEX_SELF_EXE_ENV_VAR) {
        return Ok(PathBuf::from(path));
    }
    std::env::current_exe().context("解析当前 codex 可执行文件失败")
}

async fn ensure_daemon_running(client: &FederationClient, launcher: &Path) -> Result<bool> {
    if client.ping().await.is_ok() {
        return Ok(false);
    }

    let mut command = Command::new(launcher);
    command
        .arg("zfeder")
        .arg("internal-daemon")
        .arg("--state-root")
        .arg(client.state_root())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
        .spawn()
        .with_context(|| format!("启动 zfeder daemon: {}", launcher.display()))?;

    for _ in 0..100 {
        if client.ping().await.is_ok() {
            return Ok(true);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    Err(anyhow::Error::msg(format!(
        "zfeder daemon 未在预期时间内启动: {}",
        client.endpoint_path().display()
    )))
}

async fn wait_for_endpoint_removal(endpoint_path: &Path) {
    for _ in 0..100 {
        if !endpoint_path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn print_response<T, F>(json_output: bool, value: &T, text_lines: F) -> Result<()>
where
    T: Serialize,
    F: FnOnce() -> Vec<String>,
{
    print_output(json_output, value, text_lines)
}

fn print_output<T, F>(json_output: bool, value: &T, text_lines: F) -> Result<()>
where
    T: Serialize,
    F: FnOnce() -> Vec<String>,
{
    if json_output {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        for line in text_lines() {
            println!("{line}");
        }
    }
    Ok(())
}

fn render_register_response(state_root: &Path, response: &FederationDaemonResponse) -> Vec<String> {
    match response.card.as_ref() {
        Some(card) => vec![
            format!("状态目录：{}", state_root.display()),
            format!("实例：{}", card.instance_id),
            format!("名称：{}", card.display_name),
            format!("消息：{}", response.message),
        ],
        None => vec![
            format!("状态目录：{}", state_root.display()),
            format!("消息：{}", response.message),
        ],
    }
}

fn render_peers_response(state_root: &Path, response: &FederationDaemonResponse) -> Vec<String> {
    let peers = response.peers.as_deref().unwrap_or(&[]);
    let mut lines = vec![
        format!("状态目录：{}", state_root.display()),
        format!("peer 数量：{}", peers.len()),
    ];
    for peer in peers {
        lines.push(format!(
            "{} {} {} {}",
            peer.instance_id,
            peer.display_name,
            peer.role.as_deref().unwrap_or("-"),
            peer.cwd.display()
        ));
    }
    lines
}

fn render_send_response(state_root: &Path, response: &FederationDaemonResponse) -> Vec<String> {
    match response.ack.as_ref() {
        Some(ack) => vec![
            format!("状态目录：{}", state_root.display()),
            format!("信封：{}", ack.envelope_id),
            format!("ack：{:?}", ack.state),
            format!("消息：{}", response.message),
        ],
        None => vec![
            format!("状态目录：{}", state_root.display()),
            format!("消息：{}", response.message),
        ],
    }
}

fn render_inbox_response(state_root: &Path, response: &FederationDaemonResponse) -> Vec<String> {
    let envelopes = response.envelopes.as_deref().unwrap_or(&[]);
    let mut lines = vec![
        format!("状态目录：{}", state_root.display()),
        format!("信封数量：{}", envelopes.len()),
    ];
    for envelope in envelopes {
        let description = match &envelope.payload {
            EnvelopePayload::TextTask { text } => text.as_str(),
            EnvelopePayload::TextResult { text, .. } => text.as_str(),
        };
        lines.push(format!(
            "{} {} {}",
            envelope.envelope_id, envelope.sender, description
        ));
    }
    lines
}

fn render_ack_response(state_root: &Path, response: &FederationDaemonResponse) -> Vec<String> {
    match response.ack.as_ref() {
        Some(ack) => vec![
            format!("状态目录：{}", state_root.display()),
            format!("信封：{}", ack.envelope_id),
            format!("ack：{:?}", ack.state),
            format!("消息：{}", response.message),
        ],
        None => vec![
            format!("状态目录：{}", state_root.display()),
            format!("消息：{}", response.message),
        ],
    }
}

fn parse_instance_id(value: Option<&str>) -> Result<InstanceId> {
    match value {
        Some(value) => parse_required_instance_id(value),
        None => Ok(InstanceId::default()),
    }
}

fn parse_optional_instance_id(value: Option<&str>) -> Result<Option<InstanceId>> {
    value.map(parse_required_instance_id).transpose()
}

fn parse_required_instance_id(value: &str) -> Result<InstanceId> {
    InstanceId::try_from(value).map_err(anyhow::Error::from)
}

fn parse_envelope_id(value: Option<&str>) -> Result<EnvelopeId> {
    match value {
        Some(value) => parse_required_envelope_id(value),
        None => Ok(EnvelopeId::default()),
    }
}

fn parse_required_envelope_id(value: &str) -> Result<EnvelopeId> {
    EnvelopeId::try_from(value).map_err(anyhow::Error::from)
}

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

impl From<AckStateArg> for AckState {
    fn from(value: AckStateArg) -> Self {
        match value {
            AckStateArg::Accepted => AckState::Accepted,
            AckStateArg::Delivered => AckState::Delivered,
            AckStateArg::Rejected => AckState::Rejected,
            AckStateArg::Expired => AckState::Expired,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use clap::Parser;
    use codex_federation_client::FederationClient;
    use codex_federation_daemon::FederationDaemon;
    use codex_federation_protocol::AckState;
    use codex_federation_protocol::EnvelopePayload;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use super::AckArgs;
    use super::InboxArgs;
    use super::PeersArgs;
    use super::RegisterArgs;
    use super::SendArgs;
    use super::ZfederCli;
    use super::execute_ack;
    use super::execute_inbox;
    use super::execute_peers;
    use super::execute_register;
    use super::execute_send;

    #[test]
    fn federation_register_cli_parses_required_fields() {
        let cli = ZfederCli::try_parse_from([
            "federation",
            "register",
            "--name",
            "planner",
            "--role",
            "writer",
        ])
        .expect("cli should parse");

        let super::ZfederSubcommand::Register(RegisterArgs { name, role, .. }) = cli.subcommand
        else {
            panic!("expected register subcommand");
        };
        assert_eq!(name, "planner");
        assert_eq!(role.as_deref(), Some("writer"));
    }

    #[tokio::test]
    async fn federation_round_trip_commands_work_against_daemon() {
        let tempdir = TempDir::new().expect("tempdir");
        let daemon = FederationDaemon::new(tempdir.path()).expect("daemon");
        let endpoint_path = tempdir.path().join("daemon").join("endpoint");
        let daemon_task = tokio::spawn(async move { daemon.run_until_shutdown().await });
        wait_for_endpoint(&endpoint_path).await;

        let client = FederationClient::new(tempdir.path()).expect("client");
        let sender = execute_register(
            &client,
            &RegisterArgs::try_parse_from([
                "register",
                "--state-root",
                tempdir.path().to_str().expect("utf-8 path"),
                "--name",
                "sender",
                "--cwd",
                "/workspace/sender",
            ])
            .expect("register args"),
        )
        .await
        .expect("register sender");
        let sender_id = sender.card.expect("sender card").instance_id.to_string();

        let recipient = execute_register(
            &client,
            &RegisterArgs::try_parse_from([
                "register",
                "--state-root",
                tempdir.path().to_str().expect("utf-8 path"),
                "--name",
                "recipient",
                "--cwd",
                "/workspace/recipient",
            ])
            .expect("register args"),
        )
        .await
        .expect("register recipient");
        let recipient_id = recipient
            .card
            .expect("recipient card")
            .instance_id
            .to_string();

        let peers = execute_peers(
            &client,
            &PeersArgs::try_parse_from([
                "peers",
                "--state-root",
                tempdir.path().to_str().expect("utf-8 path"),
                "--requester",
                sender_id.as_str(),
                "--now",
                "1",
            ])
            .expect("peers args"),
        )
        .await
        .expect("peers");
        assert_eq!(peers.peers.expect("peers").len(), 1);

        let send = execute_send(
            &client,
            &SendArgs::try_parse_from([
                "send",
                "--state-root",
                tempdir.path().to_str().expect("utf-8 path"),
                "--sender",
                sender_id.as_str(),
                "--recipient",
                recipient_id.as_str(),
                "--text",
                "summarize repo",
                "--created-at",
                "10",
                "--expires-in",
                "60",
            ])
            .expect("send args"),
        )
        .await
        .expect("send");
        let ack = send.ack.expect("ack");
        assert_eq!(ack.state, AckState::Accepted);

        let inbox = execute_inbox(
            &client,
            &InboxArgs::try_parse_from([
                "inbox",
                "--state-root",
                tempdir.path().to_str().expect("utf-8 path"),
                "--recipient",
                recipient_id.as_str(),
                "--now",
                "11",
            ])
            .expect("inbox args"),
        )
        .await
        .expect("inbox");
        let envelopes = inbox.envelopes.expect("envelopes");
        assert_eq!(envelopes.len(), 1);
        let EnvelopePayload::TextTask { text } = &envelopes[0].payload else {
            panic!("expected text task");
        };
        assert_eq!(text, "summarize repo");

        let ack_response = execute_ack(&client, &{
            let envelope_id = ack.envelope_id.to_string();
            AckArgs::try_parse_from([
                "ack",
                "--state-root",
                tempdir.path().to_str().expect("utf-8 path"),
                "--recipient",
                recipient_id.as_str(),
                "--envelope-id",
                envelope_id.as_str(),
                "--state",
                "delivered",
                "--updated-at",
                "12",
            ])
            .expect("ack args")
        })
        .await
        .expect("ack");
        assert_eq!(
            ack_response.ack.expect("ack response").state,
            AckState::Delivered
        );

        let empty_inbox = execute_inbox(
            &client,
            &InboxArgs::try_parse_from([
                "inbox",
                "--state-root",
                tempdir.path().to_str().expect("utf-8 path"),
                "--recipient",
                recipient_id.as_str(),
                "--now",
                "13",
            ])
            .expect("inbox args"),
        )
        .await
        .expect("empty inbox");
        assert_eq!(
            empty_inbox.envelopes.unwrap_or_default(),
            Vec::<codex_federation_protocol::Envelope>::new()
        );

        client.ping().await.expect("ping");
        client
            .send(&codex_federation_protocol::FederationDaemonCommand::Shutdown)
            .await
            .expect("shutdown");
        daemon_task
            .await
            .expect("daemon join should succeed")
            .expect("daemon should exit cleanly");
    }

    async fn wait_for_endpoint(endpoint_path: &Path) {
        for _ in 0..50 {
            if endpoint_path.exists() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("endpoint should appear: {}", endpoint_path.display());
    }
}

use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::MutexGuard;

use codex_core::spawn::CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR;
use codex_mcp_server::CodexToolCallParam;
use codex_mcp_server::ExecApprovalElicitRequestParams;
use codex_mcp_server::ExecApprovalResponse;
use codex_mcp_server::PatchApprovalElicitRequestParams;
use codex_mcp_server::PatchApprovalResponse;
use codex_native_tldr::TldrConfig;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisResponse;
use codex_native_tldr::daemon::TldrDaemon;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::TldrDaemonResponse;
use codex_native_tldr::daemon::socket_path_for_project;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::session::SessionSnapshot;
use codex_protocol::protocol::FileChange;
use codex_protocol::protocol::ReviewDecision;
use codex_shell_command::parse_command;
use core_test_support::skip_if_no_network;
use mcp_test_support::McpProcess;
use mcp_test_support::create_apply_patch_sse_response;
use mcp_test_support::create_final_assistant_message_sse_response;
use mcp_test_support::create_mock_responses_server;
use mcp_test_support::create_shell_command_sse_response;
use mcp_test_support::format_with_current_shell;
use pretty_assertions::assert_eq;
use rmcp::model::JsonRpcResponse;
use rmcp::model::JsonRpcVersion2_0;
use rmcp::model::RequestId;
use serde_json::json;
use tempfile::TempDir;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::UnixListener;
use tokio::task;
use tokio::time::timeout;
use wiremock::MockServer;
// Allow ample time on slower CI or under load to avoid flakes.
const DEFAULT_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

static TLDR_ARTIFACT_TEST_LOCK: Mutex<()> = Mutex::new(());

struct TldrArtifactTestGuard {
    _local: MutexGuard<'static, ()>,
    _cross_process: File,
}

fn tldr_artifact_test_guard() -> TldrArtifactTestGuard {
    let local = match TLDR_ARTIFACT_TEST_LOCK.lock() {
        Ok(guard) => guard,
        Err(err) => panic!("tldr artifact test lock should not be poisoned: {err}"),
    };
    let lock_path = env::temp_dir().join("codex-native-tldr-artifact-tests.lock");
    let cross_process = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(file) => file,
        Err(err) => panic!("cross-process tldr artifact test lock should be opened: {err}"),
    };
    if let Err(err) = cross_process.lock() {
        panic!("cross-process tldr artifact test lock should be acquired: {err}");
    }
    TldrArtifactTestGuard {
        _local: local,
        _cross_process: cross_process,
    }
}

fn bind_test_unix_listener(socket_path: &Path) -> anyhow::Result<UnixListener> {
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(UnixListener::bind(socket_path)?)
}

/// Test that a shell command that is not on the "trusted" list triggers an
/// elicitation request to the MCP and that sending the approval runs the
/// command, as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_shell_command_approval_triggers_elicitation() {
    if env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    // Apparently `#[tokio::test]` must return `()`, so we create a helper
    // function that returns `Result` so we can use `?` in favor of `unwrap`.
    if let Err(err) = shell_command_approval_triggers_elicitation().await {
        panic!("failure: {err}");
    }
}

async fn shell_command_approval_triggers_elicitation() -> anyhow::Result<()> {
    // Use a simple, untrusted command that creates a file so we can
    // observe a side-effect.
    let workdir_for_shell_function_call = TempDir::new()?;
    let created_filename = "created_by_shell_tool.txt";
    let created_file = workdir_for_shell_function_call
        .path()
        .join(created_filename);

    let shell_command = if cfg!(windows) {
        vec![
            "New-Item".to_string(),
            "-ItemType".to_string(),
            "File".to_string(),
            "-Path".to_string(),
            created_filename.to_string(),
            "-Force".to_string(),
        ]
    } else {
        vec!["touch".to_string(), created_filename.to_string()]
    };
    let expected_shell_command =
        format_with_current_shell(&shlex::try_join(shell_command.iter().map(String::as_str))?);

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(vec![
        create_shell_command_sse_response(
            shell_command.clone(),
            Some(workdir_for_shell_function_call.path()),
            Some(5_000),
            "call1234",
        )?,
        create_final_assistant_message_sse_response("File created!")?,
    ])
    .await?;

    // Send a "codex" tool request, which should hit the responses endpoint.
    // In turn, it should reply with a tool call, which the MCP should forward
    // as an elicitation.
    let codex_request_id = mcp_process
        .send_codex_tool_call(CodexToolCallParam {
            prompt: "run `git init`".to_string(),
            ..Default::default()
        })
        .await?;
    let elicitation_request = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_request_message(),
    )
    .await??;

    assert_eq!(elicitation_request.jsonrpc, JsonRpcVersion2_0);
    assert_eq!(elicitation_request.request.method, "elicitation/create");

    let elicitation_request_id = elicitation_request.id.clone();
    let params = serde_json::from_value::<ExecApprovalElicitRequestParams>(
        elicitation_request
            .request
            .params
            .clone()
            .ok_or_else(|| anyhow::anyhow!("elicitation_request.params must be set"))?,
    )?;
    assert_eq!(
        elicitation_request.request.params,
        Some(create_expected_elicitation_request_params(
            expected_shell_command,
            workdir_for_shell_function_call.path(),
            codex_request_id.to_string(),
            params.codex_event_id.clone(),
            params.thread_id,
        )?)
    );

    // Accept the `git init` request by responding to the elicitation.
    mcp_process
        .send_response(
            elicitation_request_id,
            serde_json::to_value(ExecApprovalResponse {
                decision: ReviewDecision::Approved,
            })?,
        )
        .await?;

    // Verify task_complete notification arrives before the tool call completes.
    #[expect(clippy::expect_used)]
    let _task_complete = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_legacy_task_complete_notification(),
    )
    .await
    .expect("task_complete_notification timeout")
    .expect("task_complete_notification resp");

    // Verify the original `codex` tool call completes and that the file was created.
    let codex_response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(codex_request_id)),
    )
    .await??;
    assert_eq!(
        JsonRpcResponse {
            jsonrpc: JsonRpcVersion2_0,
            id: RequestId::Number(codex_request_id),
            result: json!({
                "content": [
                    {
                        "text": "File created!",
                        "type": "text"
                    }
                ],
                "structuredContent": {
                    "threadId": params.thread_id,
                    "content": "File created!"
                }
            }),
        },
        codex_response
    );

    assert!(created_file.is_file(), "created file should exist");

    Ok(())
}

fn create_expected_elicitation_request_params(
    command: Vec<String>,
    workdir: &Path,
    codex_mcp_tool_call_id: String,
    codex_event_id: String,
    thread_id: codex_protocol::ThreadId,
) -> anyhow::Result<serde_json::Value> {
    let expected_message = format!(
        "Allow Codex to run `{}` in `{}`?",
        shlex::try_join(command.iter().map(std::convert::AsRef::as_ref))?,
        workdir.to_string_lossy()
    );
    let codex_parsed_cmd = parse_command::parse_command(&command);
    let params_json = serde_json::to_value(ExecApprovalElicitRequestParams {
        message: expected_message,
        requested_schema: json!({"type":"object","properties":{}}),
        thread_id,
        codex_elicitation: "exec-approval".to_string(),
        codex_mcp_tool_call_id,
        codex_event_id,
        codex_command: command,
        codex_cwd: workdir.to_path_buf(),
        codex_call_id: "call1234".to_string(),
        codex_parsed_cmd,
    })?;
    Ok(params_json)
}

/// Test that patch approval triggers an elicitation request to the MCP and that
/// sending the approval applies the patch, as expected.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_patch_approval_triggers_elicitation() {
    if env::var(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR).is_ok() {
        println!(
            "Skipping test because it cannot execute when network is disabled in a Codex sandbox."
        );
        return;
    }

    if let Err(err) = patch_approval_triggers_elicitation().await {
        panic!("failure: {err}");
    }
}

async fn patch_approval_triggers_elicitation() -> anyhow::Result<()> {
    if cfg!(windows) {
        // powershell apply_patch shell calls are not parsed into apply patch approvals

        return Ok(());
    }

    let cwd = TempDir::new()?;
    let test_file = cwd.path().join("destination_file.txt");
    std::fs::write(&test_file, "original content\n")?;

    let patch_content = format!(
        "*** Begin Patch\n*** Update File: {}\n-original content\n+modified content\n*** End Patch",
        test_file.as_path().to_string_lossy()
    );

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(vec![
        create_apply_patch_sse_response(&patch_content, "call1234")?,
        create_final_assistant_message_sse_response("Patch has been applied successfully!")?,
    ])
    .await?;

    // Send a "codex" tool request that will trigger the apply_patch command
    let codex_request_id = mcp_process
        .send_codex_tool_call(CodexToolCallParam {
            cwd: Some(cwd.path().to_string_lossy().to_string()),
            prompt: "please modify the test file".to_string(),
            ..Default::default()
        })
        .await?;
    let elicitation_request = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_request_message(),
    )
    .await??;

    assert_eq!(elicitation_request.jsonrpc, JsonRpcVersion2_0);
    assert_eq!(elicitation_request.request.method, "elicitation/create");

    let elicitation_request_id = elicitation_request.id.clone();
    let params = serde_json::from_value::<PatchApprovalElicitRequestParams>(
        elicitation_request
            .request
            .params
            .clone()
            .ok_or_else(|| anyhow::anyhow!("elicitation_request.params must be set"))?,
    )?;

    let mut expected_changes = HashMap::new();
    expected_changes.insert(
        test_file.as_path().to_path_buf(),
        FileChange::Update {
            unified_diff: "@@ -1 +1 @@\n-original content\n+modified content\n".to_string(),
            move_path: None,
        },
    );

    assert_eq!(
        elicitation_request.request.params,
        Some(create_expected_patch_approval_elicitation_request_params(
            expected_changes,
            None, // No grant_root expected
            None, // No reason expected
            codex_request_id.to_string(),
            params.codex_event_id.clone(),
            params.thread_id,
        )?)
    );

    // Accept the patch approval request by responding to the elicitation
    mcp_process
        .send_response(
            elicitation_request_id,
            serde_json::to_value(PatchApprovalResponse {
                decision: ReviewDecision::Approved,
            })?,
        )
        .await?;

    // Verify the original `codex` tool call completes
    let codex_response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(codex_request_id)),
    )
    .await??;
    assert_eq!(
        JsonRpcResponse {
            jsonrpc: JsonRpcVersion2_0,
            id: RequestId::Number(codex_request_id),
            result: json!({
                "content": [
                    {
                        "text": "Patch has been applied successfully!",
                        "type": "text"
                    }
                ],
                "structuredContent": {
                    "threadId": params.thread_id,
                    "content": "Patch has been applied successfully!"
                }
            }),
        },
        codex_response
    );

    let file_contents = std::fs::read_to_string(test_file.as_path())?;
    assert_eq!(file_contents, "modified content\n");

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_codex_tool_passes_base_instructions() {
    skip_if_no_network!();

    // Apparently `#[tokio::test]` must return `()`, so we create a helper
    // function that returns `Result` so we can use `?` in favor of `unwrap`.
    if let Err(err) = codex_tool_passes_base_instructions().await {
        panic!("failure: {err}");
    }
}

async fn codex_tool_passes_base_instructions() -> anyhow::Result<()> {
    #![expect(clippy::expect_used, clippy::unwrap_used)]

    let server =
        create_mock_responses_server(vec![create_final_assistant_message_sse_response("Enjoy!")?])
            .await;

    // Run `codex mcp` with a specific config.toml.
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;
    let mut mcp_process = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp_process.initialize()).await??;

    // Send a "codex" tool request, which should hit the responses endpoint.
    let codex_request_id = mcp_process
        .send_codex_tool_call(CodexToolCallParam {
            prompt: "How are you?".to_string(),
            base_instructions: Some("You are a helpful assistant.".to_string()),
            developer_instructions: Some("Foreshadow upcoming tool calls.".to_string()),
            ..Default::default()
        })
        .await?;

    let codex_response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(codex_request_id)),
    )
    .await??;
    assert_eq!(codex_response.jsonrpc, JsonRpcVersion2_0);
    assert_eq!(codex_response.id, RequestId::Number(codex_request_id));
    assert_eq!(
        codex_response.result,
        json!({
            "content": [
                {
                    "text": "Enjoy!",
                    "type": "text"
                }
            ],
            "structuredContent": {
                "threadId": codex_response
                    .result
                    .get("structuredContent")
                    .and_then(|v| v.get("threadId"))
                    .and_then(serde_json::Value::as_str)
                    .expect("codex tool response should include structuredContent.threadId"),
                "content": "Enjoy!"
            }
        })
    );

    let requests = server.received_requests().await.unwrap();
    let request = requests[0].body_json::<serde_json::Value>()?;
    let instructions = request["instructions"]
        .as_str()
        .expect("responses request should include instructions");
    assert!(instructions.starts_with("You are a helpful assistant."));

    let developer_messages: Vec<&serde_json::Value> = request["input"]
        .as_array()
        .expect("responses request should include input items")
        .iter()
        .filter(|msg| msg.get("role").and_then(|role| role.as_str()) == Some("developer"))
        .collect();
    let developer_contents: Vec<&str> = developer_messages
        .iter()
        .filter_map(|msg| msg.get("content").and_then(serde_json::Value::as_array))
        .flat_map(|content| content.iter())
        .filter(|span| span.get("type").and_then(serde_json::Value::as_str) == Some("input_text"))
        .filter_map(|span| span.get("text").and_then(serde_json::Value::as_str))
        .collect();
    assert!(
        developer_contents
            .iter()
            .any(|content| content.contains("`sandbox_mode`")),
        "expected permissions developer message, got {developer_contents:?}"
    );
    assert!(
        developer_contents.contains(&"Foreshadow upcoming tool calls."),
        "expected developer instructions in developer messages, got {developer_contents:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_is_listed() {
    if let Err(err) = tldr_tool_is_listed().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_is_listed() -> anyhow::Result<()> {
    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process.send_list_tools_request().await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let tools = response.result["tools"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("tools/list should return an array"))?;
    assert!(
        tools
            .iter()
            .any(|tool| tool.get("name").and_then(serde_json::Value::as_str) == Some("tldr")),
        "expected `tldr` tool in tools/list, got {tools:?}"
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_tree_falls_back_to_local_engine() {
    if let Err(err) = tldr_tool_tree_falls_back_to_local_engine().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_tree_falls_back_to_local_engine() -> anyhow::Result<()> {
    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/main.rs"), "fn main() {}\n")?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(
                serde_json::json!({
                    "action": "tree",
                    "project": project.path(),
                    "language": "rust",
                    "symbol": "main"
                })
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("tool call args should be object"))?
                .clone(),
            ),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let summary = "structure summary: found 1 match(es) for `main` in 1 indexed files; function main @ src/main.rs:1-1 module [none] visibility [<none>] signature [fn main()] calls [none]";
    assert_eq!(
        response.result,
        json!({
            "content": [{
                "text": format!("tree rust via local: {summary}"),
                "type": "text"
            }],
            "isError": false,
            "structuredContent": {
                "action": "tree",
                "analysis": {
                    "kind": "ast",
                    "summary": summary
                },
                "fallbackStrategy": "structure + search",
                "language": "rust",
                "message": "daemon unavailable; used local engine",
                "project": project.path().canonicalize()?,
                "source": "local",
                "summary": summary,
                "supportLevel": "DataFlow",
                "symbol": "main"
            }
        })
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_uses_daemon_when_available() {
    if let Err(err) = tldr_tool_uses_daemon_when_available().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_uses_daemon_when_available() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = bind_test_unix_listener(&socket_path)?;
    let server_handle = task::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        match command {
            TldrDaemonCommand::Analyze { key: _, request } => {
                assert_eq!(request.kind, AnalysisKind::Ast);
            }
            other => panic!("expected analyze, got {other:?}"),
        }
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "daemon".to_string(),
            analysis: Some(AnalysisResponse {
                kind: AnalysisKind::Ast,
                summary: "daemon summary".to_string(),
            }),
            semantic: None,
            snapshot: Some(SessionSnapshot {
                cached_entries: 0,
                dirty_files: 0,
                dirty_file_threshold: 20,
                reindex_pending: false,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
            }),
            daemon_status: None,
            reindex_report: None,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("tree")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
                ("language".to_string(), json!("rust")),
                ("symbol".to_string(), json!("main")),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["source"], "daemon");
    assert_eq!(structured["summary"], "daemon summary");
    assert_eq!(structured["message"], "daemon");
    assert_eq!(structured["action"], "tree");
    assert_eq!(structured["analysis"]["kind"], "ast");
    assert_eq!(structured["analysis"]["summary"], "daemon summary");

    server_handle.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_context_exposes_analysis_payload() {
    if let Err(err) = tldr_tool_context_exposes_analysis_payload().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_context_exposes_analysis_payload() -> anyhow::Result<()> {
    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/main.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(
                serde_json::json!({
                    "action": "context",
                    "project": project.path(),
                    "language": "rust",
                    "symbol": "main"
                })
                .as_object()
                .ok_or_else(|| anyhow::anyhow!("tool call args should be object"))?
                .clone(),
            ),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "context");
    assert_eq!(structured["source"], "local");
    assert_eq!(structured["analysis"]["kind"], "call_graph");
    assert_eq!(structured["analysis"]["summary"], structured["summary"]);
    assert!(
        structured["summary"]
            .as_str()
            .is_some_and(|summary| summary.contains("context summary:"))
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_semantic_uses_daemon_when_available() {
    if let Err(err) = tldr_tool_semantic_uses_daemon_when_available().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_semantic_uses_daemon_when_available() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = bind_test_unix_listener(&socket_path)?;
    let server_handle = task::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        match command {
            TldrDaemonCommand::Semantic { request } => {
                assert_eq!(request.language, SupportedLanguage::Rust);
                assert_eq!(request.query, "auth token");
            }
            other => panic!("expected semantic, got {other:?}"),
        }
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "semantic from daemon".to_string(),
            analysis: None,
            semantic: Some(codex_native_tldr::semantic::SemanticSearchResponse {
                enabled: true,
                query: "auth token".to_string(),
                indexed_files: 1,
                truncated: false,
                matches: vec![codex_native_tldr::semantic::SemanticMatch {
                    score: 7,
                    path: PathBuf::from("src/auth.rs"),
                    line: 2,
                    snippet: "let auth_token = true;".to_string(),
                    unit: codex_native_tldr::semantic::EmbeddingUnit {
                        path: PathBuf::from("src/auth.rs"),
                        language: SupportedLanguage::Rust,
                        symbol: Some("verify_token".to_string()),
                        qualified_symbol: Some("auth::verify_token".to_string()),
                        symbol_aliases: vec![
                            "verify_token".to_string(),
                            "auth::verify_token".to_string(),
                        ],
                        kind: "function".to_string(),
                        line: 1,
                        span_end_line: 3,
                        module_path: vec!["auth".to_string()],
                        visibility: Some("pub".to_string()),
                        signature: Some("pub fn verify_token()".to_string()),
                        docs: vec!["Validates auth token".to_string()],
                        imports: vec!["use crate::auth::token;".to_string()],
                        references: vec!["Token".to_string()],
                        code_preview: "fn verify_token() {\n    let auth_token = true;\n}"
                            .to_string(),
                        calls: Vec::new(),
                        called_by: Vec::new(),
                        dependencies: vec!["src".to_string(), "auth.rs".to_string()],
                        cfg_summary: "2 lines sampled; 0 outgoing calls".to_string(),
                        dfg_summary: "contains local assignments".to_string(),
                        embedding_vector: None,
                    },
                    embedding_text: "semantic".to_string(),
                    embedding_score: Some(0.75),
                }],
                embedding_used: true,
                message: "semantic search returned 1 matches".to_string(),
            }),
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("semantic")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
                ("language".to_string(), json!("rust")),
                ("query".to_string(), json!("auth token")),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "semantic");
    assert_eq!(structured["source"], "daemon");
    assert_eq!(structured["embeddingUsed"], true);
    assert_eq!(structured["semantic"]["query"], "auth token");
    assert_eq!(structured["semantic"]["embeddingUsed"], true);
    assert_eq!(structured["matches"][0]["path"], "src/auth.rs");
    assert_eq!(structured["semantic"]["matches"][0]["path"], "src/auth.rs");
    assert!(structured["matches"][0].get("unit").is_none());
    assert!(structured["matches"][0].get("embedding_text").is_none());
    assert!(structured["semantic"]["matches"][0].get("unit").is_none());
    assert!(
        structured["semantic"]["matches"][0]
            .get("embedding_text")
            .is_none()
    );
    assert!(matches!(
        structured["matches"][0]["embedding_score"].as_f64(),
        Some(score) if score > 0.0
    ));

    server_handle.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_semantic_reuses_daemon_cache_until_notify_and_warm() {
    if let Err(err) = tldr_tool_semantic_reuses_daemon_cache_until_notify_and_warm().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_semantic_reuses_daemon_cache_until_notify_and_warm() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    std::fs::create_dir(project.path().join(".codex"))?;
    std::fs::create_dir(project.path().join("src"))?;
    std::fs::write(
        project.path().join(".codex/tldr.toml"),
        "[semantic]\nenabled = true\n",
    )?;
    let source_file = project.path().join("src/auth.rs");
    std::fs::write(
        &source_file,
        "fn verify_token() {\n    let auth_token = true;\n}\n",
    )?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let mut config = TldrConfig::for_project(canonical_project.clone());
    config.semantic = codex_native_tldr::semantic::SemanticConfig::default().with_enabled(true);
    let daemon = TldrDaemon::from_config(config);

    let listener = bind_test_unix_listener(&socket_path)?;
    let server_handle = task::spawn(async move {
        for _ in 0..6 {
            let (stream, _) = listener.accept().await?;
            let (reader, mut writer) = tokio::io::split(stream);
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await?
                .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
            let command: TldrDaemonCommand = serde_json::from_str(&line)?;
            let response = daemon.handle_command(command).await?;
            writer
                .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
                .await?;
        }
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    async fn call_tldr(
        mcp_process: &mut McpProcess,
        params: serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
        let request_id = mcp_process
            .send_named_tool_call("tldr", Some(params))
            .await?;
        let response = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
        )
        .await??;
        response.result["structuredContent"]
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))
    }

    let semantic_params = || {
        serde_json::Map::from_iter([
            ("action".to_string(), json!("semantic")),
            (
                "project".to_string(),
                json!(canonical_project.to_string_lossy()),
            ),
            ("language".to_string(), json!("rust")),
            ("query".to_string(), json!("auth_token")),
        ])
    };

    let first = call_tldr(&mut mcp_process, semantic_params()).await?;
    assert_eq!(first["source"], "daemon");
    assert_eq!(first["matches"][0]["path"], "src/auth.rs");
    assert_eq!(first["matches"][0]["snippet"], "let auth_token = true;");

    std::fs::write(
        &source_file,
        "fn verify_token() {\n    let session_cookie = true;\n}\n",
    )?;

    let second = call_tldr(&mut mcp_process, semantic_params()).await?;
    assert_eq!(second["source"], "daemon");
    assert_eq!(second["matches"][0]["snippet"], "let auth_token = true;");

    let notify = call_tldr(
        &mut mcp_process,
        serde_json::Map::from_iter([
            ("action".to_string(), json!("notify")),
            (
                "project".to_string(),
                json!(canonical_project.to_string_lossy()),
            ),
            ("path".to_string(), json!("src/auth.rs")),
        ]),
    )
    .await?;
    assert_eq!(notify["action"], "notify");
    assert_eq!(notify["snapshot"]["reindex_pending"], true);
    assert_eq!(notify["reindexReport"], serde_json::Value::Null);

    let warm = call_tldr(
        &mut mcp_process,
        serde_json::Map::from_iter([
            ("action".to_string(), json!("warm")),
            (
                "project".to_string(),
                json!(canonical_project.to_string_lossy()),
            ),
        ]),
    )
    .await?;
    assert_eq!(warm["action"], "warm");
    assert_eq!(warm["snapshot"]["reindex_pending"], false);
    assert_eq!(warm["daemonStatus"]["semantic_reindex_pending"], false);
    assert_eq!(warm["reindexReport"]["status"], "Completed");
    assert_eq!(warm["snapshot"]["last_reindex"], warm["reindexReport"]);

    let status = call_tldr(
        &mut mcp_process,
        serde_json::Map::from_iter([
            ("action".to_string(), json!("status")),
            (
                "project".to_string(),
                json!(canonical_project.to_string_lossy()),
            ),
        ]),
    )
    .await?;
    assert_eq!(status["action"], "status");
    assert_eq!(status["snapshot"]["reindex_pending"], false);
    assert_eq!(status["daemonStatus"]["semantic_reindex_pending"], false);
    assert_eq!(status["reindexReport"]["status"], "Completed");
    assert_eq!(status["snapshot"]["last_reindex"], status["reindexReport"]);
    assert_eq!(status["snapshot"]["last_reindex"], warm["reindexReport"]);

    let third = call_tldr(&mut mcp_process, semantic_params()).await?;
    assert_eq!(third["source"], "daemon");
    assert_eq!(
        third["matches"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("matches should be an array"))?
            .len(),
        0
    );

    server_handle.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_status_surfaces_last_failed_reindex_attempt() {
    if let Err(err) = tldr_tool_status_surfaces_last_failed_reindex_attempt().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_status_surfaces_last_failed_reindex_attempt() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let daemon = TldrDaemon::from_config(TldrConfig::for_project(canonical_project.clone()));
    let listener = bind_test_unix_listener(&socket_path)?;
    let server_handle = task::spawn(async move {
        for _ in 0..3 {
            let (stream, _) = listener.accept().await?;
            let (reader, mut writer) = tokio::io::split(stream);
            let mut lines = BufReader::new(reader).lines();
            let line = lines
                .next_line()
                .await?
                .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
            let command: TldrDaemonCommand = serde_json::from_str(&line)?;
            let response = daemon.handle_command(command).await?;
            writer
                .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
                .await?;
        }
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    async fn call_tldr(
        mcp_process: &mut McpProcess,
        params: serde_json::Map<String, serde_json::Value>,
    ) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
        let request_id = mcp_process
            .send_named_tool_call("tldr", Some(params))
            .await?;
        let response = timeout(
            DEFAULT_READ_TIMEOUT,
            mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
        )
        .await??;
        response.result["structuredContent"]
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))
    }

    let notify = call_tldr(
        &mut mcp_process,
        serde_json::Map::from_iter([
            ("action".to_string(), json!("notify")),
            (
                "project".to_string(),
                json!(canonical_project.to_string_lossy()),
            ),
            ("path".to_string(), json!("src/missing.rs")),
        ]),
    )
    .await?;
    assert_eq!(notify["snapshot"]["reindex_pending"], true);

    let warm = call_tldr(
        &mut mcp_process,
        serde_json::Map::from_iter([
            ("action".to_string(), json!("warm")),
            (
                "project".to_string(),
                json!(canonical_project.to_string_lossy()),
            ),
        ]),
    )
    .await?;
    assert_eq!(warm["reindexReport"]["status"], "Failed");
    assert_eq!(warm["snapshot"]["reindex_pending"], true);
    assert_eq!(warm["snapshot"]["last_reindex"], serde_json::Value::Null);
    assert_eq!(
        warm["snapshot"]["last_reindex_attempt"],
        warm["reindexReport"]
    );
    assert_eq!(warm["daemonStatus"]["semantic_reindex_pending"], true);

    let status = call_tldr(
        &mut mcp_process,
        serde_json::Map::from_iter([
            ("action".to_string(), json!("status")),
            (
                "project".to_string(),
                json!(canonical_project.to_string_lossy()),
            ),
        ]),
    )
    .await?;
    assert_eq!(status["reindexReport"]["status"], "Failed");
    assert_eq!(status["snapshot"]["reindex_pending"], true);
    assert_eq!(status["snapshot"]["last_reindex"], serde_json::Value::Null);
    assert_eq!(
        status["snapshot"]["last_reindex_attempt"],
        status["reindexReport"]
    );
    assert_eq!(status["daemonStatus"]["semantic_reindex_pending"], true);

    server_handle.await??;
    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_warm_returns_snapshot() {
    if let Err(err) = tldr_tool_warm_returns_snapshot().await {
        panic!("failure: {err}");
    }
}

#[cfg(unix)]
async fn tldr_tool_warm_returns_snapshot() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = bind_test_unix_listener(&socket_path)?;
    let server_handle = task::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        match command {
            TldrDaemonCommand::Warm => {}
            other => panic!("expected warm, got {other:?}"),
        }
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "warmed".to_string(),
            analysis: None,
            semantic: None,
            snapshot: Some(SessionSnapshot {
                cached_entries: 5,
                dirty_files: 1,
                dirty_file_threshold: 20,
                reindex_pending: false,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
            }),
            daemon_status: None,
            reindex_report: None,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("warm")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "warm");
    assert_eq!(structured["status"], "ok");
    assert_eq!(structured["message"], "warmed");
    assert_eq!(
        structured["project"],
        canonical_project.to_string_lossy().to_string()
    );
    assert_eq!(structured["snapshot"]["cached_entries"], 5);
    server_handle.await??;
    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_notify_includes_path() {
    if let Err(err) = tldr_tool_notify_includes_path().await {
        panic!("failure: {err}");
    }
}

#[cfg(unix)]
async fn tldr_tool_notify_includes_path() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let target_path = canonical_project.join("src/lib.rs");
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = bind_test_unix_listener(&socket_path)?;
    let target_path_for_server = target_path.clone();
    let server_handle = task::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        match command {
            TldrDaemonCommand::Notify { path } => {
                assert_eq!(path, target_path_for_server);
            }
            other => panic!("expected notify, got {other:?}"),
        }
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "notify ok".to_string(),
            analysis: None,
            semantic: None,
            snapshot: Some(SessionSnapshot {
                cached_entries: 0,
                dirty_files: 2,
                dirty_file_threshold: 20,
                reindex_pending: false,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
            }),
            daemon_status: None,
            reindex_report: None,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("notify")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
                ("path".to_string(), json!(target_path.to_string_lossy())),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "notify");
    assert_eq!(structured["message"], "notify ok");
    assert_eq!(structured["status"], "ok");
    server_handle.await??;
    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_snapshot_returns_snapshot() {
    if let Err(err) = tldr_tool_snapshot_returns_snapshot().await {
        panic!("failure: {err}");
    }
}

#[cfg(unix)]
async fn tldr_tool_snapshot_returns_snapshot() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = bind_test_unix_listener(&socket_path)?;
    let server_handle = task::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        match command {
            TldrDaemonCommand::Snapshot => {}
            other => panic!("expected snapshot, got {other:?}"),
        }
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "snapshot ok".to_string(),
            analysis: None,
            semantic: None,
            snapshot: Some(SessionSnapshot {
                cached_entries: 7,
                dirty_files: 0,
                dirty_file_threshold: 20,
                reindex_pending: false,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
            }),
            daemon_status: None,
            reindex_report: None,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("snapshot")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "snapshot");
    assert_eq!(structured["snapshot"]["cached_entries"], 7);
    server_handle.await??;
    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_ping_reports_status() {
    if let Err(err) = tldr_tool_ping_reports_status().await {
        panic!("failure: {err}");
    }
}

#[cfg(unix)]
async fn tldr_tool_ping_reports_status() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = bind_test_unix_listener(&socket_path)?;
    let server_handle = task::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        match command {
            TldrDaemonCommand::Ping => {}
            other => panic!("expected ping, got {other:?}"),
        }
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            semantic: None,
            snapshot: Some(SessionSnapshot {
                cached_entries: 0,
                dirty_files: 0,
                dirty_file_threshold: 20,
                reindex_pending: false,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
            }),
            daemon_status: None,
            reindex_report: None,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("ping")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "ping");
    assert_eq!(structured["status"], "ok");
    assert_eq!(structured["message"], "pong");
    server_handle.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_ping_errors_when_daemon_missing() {
    if let Err(err) = tldr_tool_ping_errors_when_daemon_missing().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_ping_errors_when_daemon_missing() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("ping")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let result = &response.result;
    assert_eq!(
        result["isError"],
        serde_json::Value::Bool(true),
        "expected error flag"
    );
    assert!(
        result.get("structuredContent").is_none(),
        "structuredContent should be absent when ping fails"
    );
    let expected_message = format!(
        "tldr tool failed: native-tldr daemon is unavailable for {}",
        canonical_project.display()
    );
    let actual_text = result["content"][0]["text"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("response content text missing"))?;
    assert_eq!(actual_text, expected_message);

    Ok(())
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_status_returns_daemon_status() {
    if let Err(err) = tldr_tool_status_returns_daemon_status().await {
        panic!("failure: {err}");
    }
}

#[cfg(unix)]
async fn tldr_tool_status_returns_daemon_status() -> anyhow::Result<()> {
    let _guard = tldr_artifact_test_guard();
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;
    let socket_path = socket_path_for_project(&canonical_project);
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }

    let listener = bind_test_unix_listener(&socket_path)?;
    let canonical_project_for_server = canonical_project.clone();
    let socket_path_for_server = socket_path.clone();
    let server_handle = task::spawn(async move {
        let (stream, _) = listener.accept().await?;
        let (reader, mut writer) = tokio::io::split(stream);
        let mut lines = BufReader::new(reader).lines();
        let line = lines
            .next_line()
            .await?
            .ok_or_else(|| anyhow::anyhow!("daemon client should send one line"))?;
        let command: TldrDaemonCommand = serde_json::from_str(&line)?;
        match command {
            TldrDaemonCommand::Status => {}
            other => panic!("expected status, got {other:?}"),
        }
        let response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "status".to_string(),
            analysis: None,
            semantic: None,
            snapshot: Some(SessionSnapshot {
                cached_entries: 2,
                dirty_files: 1,
                dirty_file_threshold: 20,
                reindex_pending: false,
                last_query_at: None,
                last_reindex: None,
                last_reindex_attempt: None,
            }),
            daemon_status: Some(codex_native_tldr::daemon::TldrDaemonStatus {
                project_root: canonical_project_for_server.clone(),
                socket_path: socket_path_for_server.clone(),
                pid_path: codex_native_tldr::daemon::pid_path_for_project(
                    &canonical_project_for_server,
                ),
                lock_path: codex_native_tldr::daemon::lock_path_for_project(
                    &canonical_project_for_server,
                ),
                socket_exists: true,
                pid_is_live: false,
                lock_is_held: true,
                healthy: false,
                stale_socket: true,
                stale_pid: false,
                health_reason: Some("daemon lock held; another process may be starting it".into()),
                recovery_hint: Some(
                    "wait for the existing daemon or release the lock manually".into(),
                ),
                semantic_reindex_pending: false,
                last_query_at: None,
                config: codex_native_tldr::daemon::TldrDaemonConfigSummary {
                    auto_start: true,
                    socket_mode: "auto".to_string(),
                    semantic_enabled: false,
                    semantic_auto_reindex_threshold: 20,
                    session_dirty_file_threshold: 20,
                },
            }),
            reindex_report: None,
        };
        writer
            .write_all(format!("{}\n", serde_json::to_string(&response)?).as_bytes())
            .await?;
        Ok::<(), anyhow::Error>(())
    });

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("status")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "status");
    assert_eq!(structured["daemonStatus"]["lock_is_held"], true);
    assert_eq!(
        structured["daemonStatus"]["health_reason"],
        "daemon lock held; another process may be starting it"
    );
    assert_eq!(
        structured["daemonStatus"]["recovery_hint"],
        "wait for the existing daemon or release the lock manually"
    );
    assert_eq!(structured["snapshot"]["cached_entries"], 2);
    server_handle.await??;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_semantic_structured_content() {
    if let Err(err) = tldr_tool_semantic_structured_content().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_semantic_structured_content() -> anyhow::Result<()> {
    let project = TempDir::new()?;
    let canonical_project = project.path().canonicalize()?;

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("semantic")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
                ("language".to_string(), json!("rust")),
                ("query".to_string(), json!("find auth")),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "semantic");
    assert_eq!(structured["language"], "rust");
    assert_eq!(structured["query"], "find auth");
    assert_eq!(structured["semantic"]["query"], "find auth");
    assert_eq!(structured["source"], "local");
    assert_eq!(
        structured["message"],
        "semantic search is disabled; enable [semantic].enabled in .codex/tldr.toml"
    );
    assert_eq!(
        structured["semantic"]["message"],
        "semantic search is disabled; enable [semantic].enabled in .codex/tldr.toml"
    );
    assert_eq!(structured["enabled"], false);
    assert_eq!(structured["embeddingUsed"], false);
    assert_eq!(structured["semantic"]["enabled"], false);
    assert_eq!(structured["semantic"]["embeddingUsed"], false);

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_tldr_tool_semantic_returns_matches_when_enabled() {
    if let Err(err) = tldr_tool_semantic_returns_matches_when_enabled().await {
        panic!("failure: {err}");
    }
}

async fn tldr_tool_semantic_returns_matches_when_enabled() -> anyhow::Result<()> {
    let project = TempDir::new()?;
    std::fs::create_dir(project.path().join(".codex"))?;
    std::fs::create_dir(project.path().join("src"))?;
    std::fs::write(
        project.path().join(".codex/tldr.toml"),
        "[semantic]\nenabled = true\n[semantic.embedding]\nenabled = true\ndimensions = 16\n",
    )?;
    std::fs::write(
        project.path().join("src/auth.rs"),
        "fn verify_token() {\n    let auth_token = true;\n}\n",
    )?;
    let canonical_project = project.path().canonicalize()?;

    let McpHandle {
        process: mut mcp_process,
        server: _server,
        dir: _dir,
    } = create_mcp_process(Vec::new()).await?;

    let request_id = mcp_process
        .send_named_tool_call(
            "tldr",
            Some(serde_json::Map::from_iter([
                ("action".to_string(), json!("semantic")),
                (
                    "project".to_string(),
                    json!(canonical_project.to_string_lossy()),
                ),
                ("language".to_string(), json!("rust")),
                ("query".to_string(), json!("auth token")),
            ])),
        )
        .await?;
    let response = timeout(
        DEFAULT_READ_TIMEOUT,
        mcp_process.read_stream_until_response_message(RequestId::Number(request_id)),
    )
    .await??;

    let structured = response.result["structuredContent"]
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("structuredContent should be an object"))?;
    assert_eq!(structured["action"], "semantic");
    assert_eq!(structured["enabled"], true);
    assert_eq!(structured["semantic"]["enabled"], true);
    assert_eq!(structured["embeddingUsed"], true);
    assert_eq!(structured["semantic"]["embeddingUsed"], true);
    assert_eq!(structured["indexedFiles"], 1);
    assert_eq!(structured["semantic"]["indexedFiles"], 1);
    assert_eq!(structured["matches"][0]["path"], "src/auth.rs");
    assert_eq!(structured["semantic"]["matches"][0]["path"], "src/auth.rs");
    assert_eq!(structured["matches"][0]["line"], 2);
    assert_eq!(
        structured["matches"][0]["snippet"],
        "let auth_token = true;"
    );
    assert_eq!(
        structured["semantic"]["matches"][0]["snippet"],
        "let auth_token = true;"
    );
    assert!(structured["matches"][0].get("unit").is_none());
    assert!(structured["matches"][0].get("embedding_text").is_none());
    assert!(structured["semantic"]["matches"][0].get("unit").is_none());
    assert!(
        structured["semantic"]["matches"][0]
            .get("embedding_text")
            .is_none()
    );
    assert!(matches!(
        structured["matches"][0]["embedding_score"].as_f64(),
        Some(score) if score > 0.0
    ));

    Ok(())
}

fn create_expected_patch_approval_elicitation_request_params(
    changes: HashMap<PathBuf, FileChange>,
    grant_root: Option<PathBuf>,
    reason: Option<String>,
    codex_mcp_tool_call_id: String,
    codex_event_id: String,
    thread_id: codex_protocol::ThreadId,
) -> anyhow::Result<serde_json::Value> {
    let mut message_lines = Vec::new();
    if let Some(r) = &reason {
        message_lines.push(r.clone());
    }
    message_lines.push("Allow Codex to apply proposed code changes?".to_string());
    let params_json = serde_json::to_value(PatchApprovalElicitRequestParams {
        message: message_lines.join("\n"),
        requested_schema: json!({"type":"object","properties":{}}),
        thread_id,
        codex_elicitation: "patch-approval".to_string(),
        codex_mcp_tool_call_id,
        codex_event_id,
        codex_reason: reason,
        codex_grant_root: grant_root,
        codex_changes: changes,
        codex_call_id: "call1234".to_string(),
    })?;

    Ok(params_json)
}

/// This handle is used to ensure that the MockServer and TempDir are not dropped while
/// the McpProcess is still running.
pub struct McpHandle {
    pub process: McpProcess,
    /// Retain the server for the lifetime of the McpProcess.
    #[allow(dead_code)]
    server: MockServer,
    /// Retain the temporary directory for the lifetime of the McpProcess.
    #[allow(dead_code)]
    dir: TempDir,
}

async fn create_mcp_process(responses: Vec<String>) -> anyhow::Result<McpHandle> {
    let server = create_mock_responses_server(responses).await;
    let codex_home = TempDir::new()?;
    create_config_toml(codex_home.path(), &server.uri())?;
    let mut mcp_process = McpProcess::new(codex_home.path()).await?;
    timeout(DEFAULT_READ_TIMEOUT, mcp_process.initialize()).await??;
    Ok(McpHandle {
        process: mcp_process,
        server,
        dir: codex_home,
    })
}

/// Create a Codex config that uses the mock server as the model provider.
/// It also uses `approval_policy = "untrusted"` so that we exercise the
/// elicitation code path for shell commands.
fn create_config_toml(codex_home: &Path, server_uri: &str) -> std::io::Result<()> {
    let config_toml = codex_home.join("config.toml");
    std::fs::write(
        config_toml,
        format!(
            r#"
model = "mock-model"
approval_policy = "untrusted"
sandbox_mode = "danger-full-access"

model_provider = "mock_provider"

[model_providers.mock_provider]
name = "Mock provider for test"
base_url = "{server_uri}/v1"
wire_api = "responses"
request_max_retries = 0
stream_max_retries = 0

[features]
"#
        ),
    )
}

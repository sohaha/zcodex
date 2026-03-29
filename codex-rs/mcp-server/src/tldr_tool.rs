use anyhow::Result;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::daemon_health;
use codex_native_tldr::daemon::daemon_lock_is_held;
use codex_native_tldr::daemon::launch_lock_path_for_project as native_launch_lock_path_for_project;
use codex_native_tldr::daemon::pid_path_for_project;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::daemon::socket_path_for_project;
use codex_native_tldr::lifecycle::DaemonLifecycleManager;
use codex_native_tldr::load_tldr_config;
use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::run_tldr_tool_with_hooks;
use codex_native_tldr::tool_api::tldr_tool_output_schema;
use once_cell::sync::Lazy;
use rmcp::model::CallToolResult;
use rmcp::model::Content;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use schemars::r#gen::SchemaSettings;
use std::ffi::OsStr;
use std::ffi::OsString;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use tokio::process::Command;

pub(crate) fn create_tool_for_tldr_tool_call_param() -> Tool {
    let schema = SchemaSettings::draft2019_09()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<TldrToolCallParam>();
    let input_schema = create_tool_input_schema(schema, "TLDR tool schema should serialize");

    Tool {
        name: "tldr".into(),
        title: Some("Native TLDR".to_string()),
        description: Some(
            "Structured code context analysis via native-tldr with daemon-first execution.".into(),
        ),
        input_schema,
        output_schema: Some(match tldr_tool_output_schema() {
            serde_json::Value::Object(map) => Arc::new(map),
            _ => unreachable!("json literal must be an object"),
        }),
        annotations: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

pub(crate) async fn run_tldr_tool(arguments: Option<JsonObject>) -> CallToolResult {
    let args = match arguments.map(serde_json::Value::Object) {
        Some(json_val) => match serde_json::from_value::<TldrToolCallParam>(json_val) {
            Ok(args) => args,
            Err(err) => return error_result(format!("Failed to parse tldr tool arguments: {err}")),
        },
        None => return error_result("Missing arguments for tldr tool-call.".to_string()),
    };

    run_tldr_tool_with_mcp_hooks(
        args,
        |project_root, command| Box::pin(query_daemon(project_root, command)),
        |project_root| Box::pin(ensure_daemon_running(project_root)),
    )
    .await
}

async fn run_tldr_tool_with_mcp_hooks<Q, E>(
    args: TldrToolCallParam,
    query: Q,
    ensure_running: E,
) -> CallToolResult
where
    Q: for<'a> Fn(
        &'a std::path::Path,
        &'a TldrDaemonCommand,
    ) -> codex_native_tldr::tool_api::QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a std::path::Path) -> codex_native_tldr::tool_api::EnsureDaemonFuture<'a>,
{
    match run_tldr_tool_with_hooks(args, query, ensure_running).await {
        Ok(result) => success_result(result.text, result.structured_content),
        Err(err) => error_result(format!("tldr tool failed: {err}")),
    }
}

fn success_result(text: String, structured_content: serde_json::Value) -> CallToolResult {
    CallToolResult {
        content: vec![Content::text(text)],
        structured_content: Some(structured_content),
        is_error: Some(false),
        meta: None,
    }
}

fn error_result(text: String) -> CallToolResult {
    let structured_content = tldr_error_structured_content(&text);
    CallToolResult {
        content: vec![Content::text(text)],
        structured_content,
        is_error: Some(true),
        meta: None,
    }
}

fn tldr_error_structured_content(text: &str) -> Option<serde_json::Value> {
    if text.contains("native-tldr daemon is unavailable for") {
        return Some(serde_json::json!({
            "structuredFailure": {
                "error_type": "daemon_unavailable",
                "reason": text,
                "retryable": true,
                "retry_hint": "start the daemon or retry once it becomes healthy"
            },
            "degradedMode": {
                "is_degraded": true,
                "mode": "unavailable",
                "fallback_path": "none",
                "reason": "daemon-only action requires a live daemon"
            }
        }));
    }

    None
}

static DAEMON_LIFECYCLE_MANAGER: Lazy<DaemonLifecycleManager> =
    Lazy::new(DaemonLifecycleManager::default);

async fn ensure_daemon_running(project_root: &Path) -> Result<bool> {
    if !cfg!(unix) {
        return Ok(false);
    }
    if !load_tldr_config(project_root)?.daemon.auto_start {
        return Ok(false);
    }

    DAEMON_LIFECYCLE_MANAGER
        .ensure_running_with_launcher_lock(
            project_root,
            daemon_metadata_looks_alive_with_launcher_lock,
            cleanup_stale_artifacts,
            daemon_lock_is_held,
            try_open_launcher_lock,
            |_project_root| {},
            |project_root| Box::pin(spawn_native_tldr_daemon(project_root)),
        )
        .await
}

fn daemon_launcher_command(project_root: &Path) -> Result<Command> {
    let launcher = resolve_codex_launcher()?;
    let mut command = Command::new(launcher);
    command.args(daemon_launcher_args(project_root));
    Ok(command)
}

fn daemon_launcher_args(project_root: &Path) -> [OsString; 4] {
    [
        OsString::from("tldr"),
        OsString::from("internal-daemon"),
        OsString::from("--project"),
        project_root.as_os_str().to_os_string(),
    ]
}

fn resolve_codex_launcher() -> Result<PathBuf> {
    let current_exe = std::env::current_exe()?;
    if current_exe.file_stem() == Some(OsStr::new("codex")) {
        return Ok(current_exe);
    }

    if let Some(parent) = current_exe.parent() {
        for name in codex_binary_names() {
            let candidate = parent.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Some(candidate) = find_binary_in_path(codex_binary_names()) {
        return Ok(candidate);
    }

    anyhow::bail!("unable to locate `codex` binary for native-tldr daemon auto-start")
}

fn codex_binary_names() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &["codex.exe", "codex"]
    }
    #[cfg(not(windows))]
    {
        &["codex"]
    }
}

fn find_binary_in_path(names: &[&str]) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path_var) {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn daemon_metadata_looks_alive_with_launcher_lock(
    project_root: &Path,
    ignore_launcher_lock: bool,
) -> bool {
    match daemon_health(project_root) {
        Ok(health) => {
            if health.healthy {
                return true;
            }
            if !ignore_launcher_lock && launcher_lock_is_held(project_root).unwrap_or(false) {
                return false;
            }
            if health.should_cleanup_artifacts() {
                cleanup_stale_artifacts(project_root);
            }
            false
        }
        Err(_) => false,
    }
}

async fn spawn_native_tldr_daemon(project_root: &Path) -> Result<bool> {
    let mut child = daemon_launcher_command(project_root)?
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    Ok(true)
}

fn cleanup_stale_artifacts(project_root: &Path) {
    if launcher_lock_is_held(project_root).unwrap_or(false) {
        return;
    }

    let Ok(health) = daemon_health(project_root) else {
        return;
    };
    if !health.should_cleanup_artifacts() {
        return;
    }
    cleanup_file_if_exists(socket_path_for_project(project_root));
    cleanup_file_if_exists(pid_path_for_project(project_root));
}

fn try_open_launcher_lock(project_root: &Path) -> Result<Option<File>> {
    let lock_path = native_launch_lock_path_for_project(project_root);
    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)?;

    match lock_file.try_lock() {
        Ok(()) => Ok(Some(lock_file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(err) => Err(err.into()),
    }
}

fn launcher_lock_is_held(project_root: &Path) -> Result<bool> {
    Ok(try_open_launcher_lock(project_root)?.is_none())
}

fn cleanup_file_if_exists(path: PathBuf) {
    if let Err(err) = std::fs::remove_file(&path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        let _ = err;
    }
}

fn create_tool_input_schema(
    schema: schemars::schema::RootSchema,
    panic_message: &str,
) -> Arc<JsonObject> {
    #[expect(clippy::expect_used)]
    let schema_value = serde_json::to_value(&schema).expect(panic_message);
    let mut schema_object = match schema_value {
        serde_json::Value::Object(object) => object,
        _ => panic!("tool schema should serialize to a JSON object"),
    };

    let mut input_schema = JsonObject::new();
    for key in ["properties", "required", "type", "$defs", "definitions"] {
        if let Some(value) = schema_object.remove(key) {
            input_schema.insert(key.to_string(), value);
        }
    }

    Arc::new(input_schema)
}

#[cfg(test)]
mod tests {
    use super::cleanup_stale_artifacts;
    use super::create_tool_for_tldr_tool_call_param;
    use super::daemon_metadata_looks_alive_with_launcher_lock;
    use super::ensure_daemon_running;
    use super::launcher_lock_is_held;
    use super::run_tldr_tool_with_mcp_hooks;
    use codex_native_tldr::daemon::TldrDaemonCommand;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use codex_native_tldr::daemon::pid_path_for_project;
    use codex_native_tldr::daemon::socket_path_for_project;
    use codex_native_tldr::tool_api::TldrToolAction;
    use codex_native_tldr::tool_api::TldrToolCallParam;
    use codex_native_tldr::tool_api::TldrToolLanguage;
    use codex_native_tldr::tool_api::query_daemon_with_hooks;
    use codex_native_tldr::tool_api::tldr_tool_output_schema;
    use pretty_assertions::assert_eq;
    use std::fs::OpenOptions;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tempfile::tempdir;

    #[test]
    fn verify_tldr_tool_json_schema() {
        let tool = create_tool_for_tldr_tool_call_param();
        let tool_json = serde_json::to_value(&tool).expect("tool serializes");
        assert_eq!(tool_json["name"], "tldr");
        assert_eq!(tool_json["title"], "Native TLDR");
        assert_eq!(
            tool_json["description"],
            "Structured code context analysis via native-tldr with daemon-first execution."
        );
        assert_eq!(tool_json["inputSchema"]["type"], "object");
        assert_eq!(
            tool_json["inputSchema"]["required"],
            serde_json::json!(["action"])
        );
        assert_eq!(
            tool_json["inputSchema"]["properties"]["action"]["enum"],
            serde_json::json!([
                "structure",
                "search",
                "extract",
                "imports",
                "importers",
                "context",
                "impact",
                "calls",
                "dead",
                "arch",
                "change-impact",
                "cfg",
                "dfg",
                "slice",
                "semantic",
                "diagnostics",
                "doctor",
                "ping",
                "warm",
                "snapshot",
                "status",
                "notify"
            ])
        );
        assert_eq!(tool_json["outputSchema"], tldr_tool_output_schema());
        assert_eq!(
            tool_json["outputSchema"]["oneOf"].as_array().map(Vec::len),
            Some(8)
        );
        assert_eq!(
            tool_json["outputSchema"]["$defs"]["analysisResult"]["properties"]["action"]["enum"],
            serde_json::json!([
                "structure",
                "extract",
                "context",
                "impact",
                "calls",
                "dead",
                "arch",
                "change-impact",
                "cfg",
                "dfg",
                "slice"
            ])
        );
        assert_eq!(
            tool_json["outputSchema"]["$defs"]["semanticResult"]["properties"]["action"]["const"],
            "semantic"
        );
        assert_eq!(
            tool_json["outputSchema"]["$defs"]["daemonResult"]["properties"]["action"]["enum"],
            serde_json::json!(["ping", "warm", "snapshot", "status", "notify"])
        );
    }

    #[test]
    fn tldr_tool_param_serializes_camel_case_fields() {
        let value = serde_json::to_value(TldrToolCallParam {
            action: TldrToolAction::Semantic,
            project: Some("/tmp/project".to_string()),
            language: Some(TldrToolLanguage::Typescript),
            symbol: None,
            query: Some("where is auth".to_string()),
            module: None,
            path: None,
            line: None,
            paths: None,

            ..Default::default()
        })
        .expect("tool call should serialize");

        assert_eq!(
            value,
            serde_json::json!({
                "action": "semantic",
                "project": "/tmp/project",
                "language": "typescript",
                "query": "where is auth"
            })
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_impact_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary =
            "impact summary: 1 symbols across 1 files (1 touched paths); dependency edges=1";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Impact,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: Some("AuthService".to_string()),
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "impact ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Impact,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: Some("AuthService".to_string()),
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: Vec::new(),
                                nodes: Vec::new(),
                                edges: vec![codex_native_tldr::api::AnalysisEdgeDetail {
                                    from: "AuthService".to_string(),
                                    to: "auth::audit".to_string(),
                                    kind: "depends_on".to_string(),
                                }],
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result_json["content"][0]["type"], "text");
        assert_eq!(
            result_json["content"][0]["text"],
            format!("impact rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "impact");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["analysis"]["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "impact");
        assert_eq!(
            structured["analysis"]["details"]["symbol_query"],
            "AuthService"
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_calls_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary = "calls summary: 1 call edge across 1 file";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Calls,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "calls ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Calls,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: None,
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: Vec::new(),
                                nodes: Vec::new(),
                                edges: Vec::new(),
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            format!("calls rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "calls");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "calls");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_dead_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary = "dead summary: 0 callers detected";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Dead,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "dead ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Dead,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: None,
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: Vec::new(),
                                nodes: Vec::new(),
                                edges: Vec::new(),
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            format!("dead rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "dead");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "dead");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_arch_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary = "arch summary: entry=1 middle=1 leaf=1";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Arch,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "arch ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Arch,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 3,
                                symbol_query: None,
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: Vec::new(),
                                nodes: Vec::new(),
                                edges: Vec::new(),
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            format!("arch rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "arch");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "arch");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_change_impact_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary =
            "change-impact summary: 1 changed paths -> 2 impacted symbols across 1 indexed files";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::ChangeImpact,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: Some(vec!["src/lib.rs".to_string()]),

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "change-impact ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::ChangeImpact,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 2,
                                symbol_query: None,
                                truncated: false,
                                change_paths: vec!["src/lib.rs".to_string()],
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: vec![codex_native_tldr::api::AnalysisFileDetail {
                                    path: "src/lib.rs".to_string(),
                                    symbol_count: 2,
                                    kinds: vec![codex_native_tldr::api::AnalysisCountDetail {
                                        name: "function".to_string(),
                                        count: 2,
                                    }],
                                }],
                                nodes: Vec::new(),
                                edges: Vec::new(),
                                symbol_index: Vec::new(),
                                units: vec![
                                    codex_native_tldr::api::AnalysisUnitDetail {
                                        path: "src/lib.rs".to_string(),
                                        line: 1,
                                        span_end_line: 1,
                                        symbol: Some("validate".to_string()),
                                        qualified_symbol: None,
                                        kind: "function".to_string(),
                                        module_path: Vec::new(),
                                        visibility: None,
                                        signature: Some("fn validate()".to_string()),
                                        calls: Vec::new(),
                                        called_by: vec!["login".to_string()],
                                        references: Vec::new(),
                                        imports: Vec::new(),
                                        dependencies: Vec::new(),
                                        cfg_summary: "cfg".to_string(),
                                        dfg_summary: "dfg".to_string(),
                                    },
                                    codex_native_tldr::api::AnalysisUnitDetail {
                                        path: "src/lib.rs".to_string(),
                                        line: 3,
                                        span_end_line: 4,
                                        symbol: Some("login".to_string()),
                                        qualified_symbol: None,
                                        kind: "function".to_string(),
                                        module_path: Vec::new(),
                                        visibility: None,
                                        signature: Some("fn login()".to_string()),
                                        calls: vec!["validate".to_string()],
                                        called_by: Vec::new(),
                                        references: Vec::new(),
                                        imports: Vec::new(),
                                        dependencies: Vec::new(),
                                        cfg_summary: "cfg".to_string(),
                                        dfg_summary: "dfg".to_string(),
                                    },
                                ],
                            }),
                        }),
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            format!("change-impact rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "change-impact");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["paths"], serde_json::json!(["src/lib.rs"]));
        assert_eq!(structured["analysis"]["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "change_impact");
        assert_eq!(
            structured["analysis"]["details"]["change_paths"],
            serde_json::json!(["src/lib.rs"])
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_cfg_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary = "cfg summary: 1 symbols across 1 files; sample: AuthService [cfg]";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Cfg,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: Some("AuthService".to_string()),
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "cfg ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Cfg,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: Some("AuthService".to_string()),
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: Vec::new(),
                                nodes: Vec::new(),
                                edges: Vec::new(),
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            format!("cfg rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "cfg");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "cfg");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_extract_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary = "extract summary: src/lib.rs => 1 symbols (1 function); imports=0, references=0; sample: main:1-3";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Extract,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(codex_native_tldr::daemon::TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "extract ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Extract,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: None,
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: vec![codex_native_tldr::api::AnalysisFileDetail {
                                    path: "src/lib.rs".to_string(),
                                    symbol_count: 1,
                                    kinds: vec![codex_native_tldr::api::AnalysisCountDetail {
                                        name: "function".to_string(),
                                        count: 1,
                                    }],
                                }],
                                nodes: Vec::new(),
                                edges: Vec::new(),
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("tool result serializes");
        let structured = result
            .structured_content
            .clone()
            .expect("structured content should be present");
        assert_eq!(
            result_json["content"][0]["text"],
            format!("extract rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "extract");
        assert_eq!(structured["summary"], summary);
        assert_eq!(structured["analysis"]["kind"], "extract");
        assert_eq!(structured["path"], "src/lib.rs");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_imports_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Imports,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(codex_native_tldr::daemon::TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "imports ready: src/lib.rs".to_string(),
                        analysis: None,
                        imports: Some(codex_native_tldr::api::ImportsResponse {
                            language: codex_native_tldr::lang_support::SupportedLanguage::Rust,
                            path: "src/lib.rs".to_string(),
                            indexed_files: 1,
                            imports: vec!["use crate::auth::token;".to_string()],
                        }),
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("tool result should serialize");
        let structured = result
            .structured_content
            .clone()
            .expect("structured content should be present");
        assert_eq!(
            result_json["content"][0]["text"],
            "imports rust via daemon: 1 imports"
        );
        assert_eq!(structured["action"], "imports");
        assert_eq!(structured["path"], "src/lib.rs");
        assert_eq!(
            structured["imports"]["imports"],
            serde_json::json!(["use crate::auth::token;"])
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_slice_summary_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let summary = "slice summary: backward slice for src/lib.rs:login:4 -> 3 lines [1, 3, 4]";
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Slice,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: Some("login".to_string()),
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: Some(4),
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(codex_native_tldr::daemon::TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "slice ready".to_string(),
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Slice,
                            summary: summary.to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: Some("login".to_string()),
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: Some(codex_native_tldr::api::AnalysisSliceTarget {
                                    path: "src/lib.rs".to_string(),
                                    symbol: Some("login".to_string()),
                                    line: 4,
                                    direction: "backward".to_string(),
                                }),
                                slice_lines: vec![1, 3, 4],
                                overview: codex_native_tldr::api::AnalysisOverviewDetail::default(),
                                files: Vec::new(),
                                nodes: Vec::new(),
                                edges: Vec::new(),
                                symbol_index: Vec::new(),
                                units: Vec::new(),
                            }),
                        }),
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("tool result serializes");
        let structured = result
            .structured_content
            .clone()
            .expect("structured content should be present");
        assert_eq!(
            result_json["content"][0]["text"],
            format!("slice rust via daemon: {summary}")
        );
        assert_eq!(structured["action"], "slice");
        assert_eq!(structured["line"], 4);
        assert_eq!(structured["analysis"]["kind"], "slice");
        assert_eq!(
            structured["analysis"]["details"]["slice_lines"],
            serde_json::json!([1, 3, 4])
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_search_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let pattern = "auth".to_string();
        let response = codex_native_tldr::api::SearchResponse {
            pattern: pattern.clone(),
            indexed_files: 1,
            truncated: false,
            matches: vec![codex_native_tldr::api::SearchMatch {
                path: "src/main.rs".to_string(),
                line: 1,
                content: "found auth".to_string(),
            }],
        };
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Search,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: Some(pattern.clone()),
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                let response = response.clone();
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "search ready".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: Some(response),
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            "search via daemon: 1 matches"
        );
        assert_eq!(structured["action"], "search");
        assert_eq!(structured["search"]["pattern"], pattern);
        assert_eq!(structured["search"]["matches"][0]["path"], "src/main.rs");
        assert_eq!(structured["search"]["matches"][0]["line"], 1);
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_diagnostics_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let diagnostics = codex_native_tldr::api::DiagnosticsResponse {
            language: codex_native_tldr::lang_support::SupportedLanguage::Rust,
            path: "src/main.rs".to_string(),
            tools: vec![codex_native_tldr::api::DiagnosticToolStatus {
                tool: "cargo-check".to_string(),
                available: true,
            }],
            diagnostics: vec![codex_native_tldr::api::DiagnosticItem {
                path: "src/main.rs".to_string(),
                line: 1,
                column: 1,
                severity: codex_native_tldr::api::DiagnosticSeverity::Error,
                message: "failed".to_string(),
                code: Some("E001".to_string()),
                source: "cargo-check".to_string(),
            }],
            message: "diagnostics reported 1 issue".to_string(),
        };
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Diagnostics,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: Some("src/main.rs".to_string()),
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                let diagnostics = diagnostics.clone();
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "diagnostics ready".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: Some(diagnostics),
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            "diagnostics rust via daemon: 1 issues"
        );
        assert_eq!(structured["action"], "diagnostics");
        assert_eq!(structured["diagnostics"]["path"], "src/main.rs");
        assert_eq!(
            structured["diagnostics"]["diagnostics"][0]["message"],
            "failed"
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_status_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Status,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "status".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result_json["content"][0]["text"], "status: status");
        assert_eq!(structured["action"], "status");
        assert_eq!(structured["status"], "ok");
        assert_eq!(structured["message"], "status");
        assert_eq!(structured["structuredFailure"], serde_json::Value::Null);
        assert_eq!(structured["degradedMode"], serde_json::Value::Null);
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_ping_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Ping,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "pong".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result_json["content"][0]["text"], "ping: pong");
        assert_eq!(structured["action"], "ping");
        assert_eq!(structured["status"], "ok");
        assert_eq!(structured["message"], "pong");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_warm_snapshot_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Warm,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "already warm".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                            cached_entries: 0,
                            dirty_files: 0,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            background_reindex_in_progress: false,
                            last_query_at: None,
                            last_reindex: None,
                            last_reindex_attempt: None,
                            last_warm: None,
                        }),
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result_json["content"][0]["text"], "warm: already warm");
        assert_eq!(structured["action"], "warm");
        assert_eq!(structured["snapshot"]["dirty_files"], 0);
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_snapshot_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Snapshot,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "snapshot".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                            cached_entries: 2,
                            dirty_files: 1,
                            dirty_file_threshold: 20,
                            reindex_pending: true,
                            background_reindex_in_progress: false,
                            last_query_at: None,
                            last_reindex: None,
                            last_reindex_attempt: None,
                            last_warm: None,
                        }),
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(result_json["content"][0]["text"], "snapshot: snapshot");
        assert_eq!(structured["action"], "snapshot");
        assert_eq!(structured["snapshot"]["cached_entries"], 2);
        assert_eq!(structured["snapshot"]["dirty_files"], 1);
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_notify_snapshot_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Notify,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "marked src/lib.rs dirty".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                            cached_entries: 1,
                            dirty_files: 1,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            background_reindex_in_progress: false,
                            last_query_at: None,
                            last_reindex: None,
                            last_reindex_attempt: None,
                            last_warm: None,
                        }),
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(result.is_error, Some(false));
        assert_eq!(
            result_json["content"][0]["text"],
            "notify: marked src/lib.rs dirty"
        );
        assert_eq!(structured["action"], "notify");
        assert_eq!(structured["snapshot"]["dirty_files"], 1);
        assert_eq!(structured["message"], "marked src/lib.rs dirty");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_preserves_status_detail_fields() {
        let tempdir = tempdir().expect("tempdir should exist");
        let report = codex_native_tldr::semantic::SemanticReindexReport {
            status: codex_native_tldr::semantic::SemanticReindexStatus::Completed,
            languages: vec![codex_native_tldr::lang_support::SupportedLanguage::Rust],
            indexed_files: 2,
            indexed_units: 3,
            truncated: false,
            started_at: std::time::SystemTime::UNIX_EPOCH,
            finished_at: std::time::SystemTime::UNIX_EPOCH,
            message: "done".to_string(),
            embedding_enabled: true,
            embedding_dimensions: 256,
        };
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Status,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| {
                let report = report.clone();
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "status".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        search: None,
                        diagnostics: None,
                        semantic: None,
                        snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                            cached_entries: 1,
                            dirty_files: 0,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            background_reindex_in_progress: false,
                            last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                            last_reindex: Some(report.clone()),
                            last_reindex_attempt: Some(report.clone()),
                            last_warm: Some(codex_native_tldr::session::WarmReport {
                                status: codex_native_tldr::session::WarmStatus::Loaded,
                                languages: vec![
                                    codex_native_tldr::lang_support::SupportedLanguage::Rust,
                                ],
                                started_at: std::time::SystemTime::UNIX_EPOCH,
                                finished_at: std::time::SystemTime::UNIX_EPOCH,
                                message: "warm loaded 1 language indexes into daemon cache"
                                    .to_string(),
                            }),
                        }),
                        daemon_status: Some(codex_native_tldr::daemon::TldrDaemonStatus {
                            project_root: std::path::PathBuf::from("/tmp/project"),
                            socket_path: std::path::PathBuf::from("/tmp/project.sock"),
                            pid_path: std::path::PathBuf::from("/tmp/project.pid"),
                            lock_path: std::path::PathBuf::from("/tmp/project.lock"),
                            socket_exists: true,
                            pid_is_live: true,
                            lock_is_held: true,
                            healthy: true,
                            stale_socket: false,
                            stale_pid: false,
                            health_reason: None,
                            recovery_hint: None,
                            semantic_reindex_pending: false,
                            semantic_reindex_in_progress: false,
                            last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                            config: codex_native_tldr::daemon::TldrDaemonConfigSummary {
                                auto_start: true,
                                socket_mode: "unix".to_string(),
                                semantic_enabled: true,
                                semantic_auto_reindex_threshold: 20,
                                session_dirty_file_threshold: 20,
                                session_idle_timeout_secs: 1800,
                            },
                        }),
                        reindex_report: Some(report),
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let structured = result
            .structured_content
            .as_ref()
            .expect("structured content should be present");

        assert_eq!(structured["daemonStatus"]["healthy"], true);
        assert_eq!(structured["structuredFailure"], serde_json::Value::Null);
        assert_eq!(structured["degradedMode"], serde_json::Value::Null);
        assert_eq!(structured["reindexReport"]["status"], "Completed");
        assert_eq!(structured["snapshot"]["last_warm"]["status"], "Loaded");
        assert_eq!(
            structured["snapshot"]["last_reindex_attempt"]["status"],
            "Completed"
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_surfaces_missing_language_error() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Structure,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| Box::pin(async move { Ok(None) }),
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let result_json = serde_json::to_value(&result).expect("call tool result should serialize");
        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            result_json["content"][0]["text"],
            "tldr tool failed: `language` is required for action=structure"
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_mcp_hooks_surfaces_daemon_unavailable_error() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_mcp_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Status,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,

                ..Default::default()
            },
            |_project_root, _command| Box::pin(async move { Ok(None) }),
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await;

        let text = serde_json::to_value(&result).expect("call tool result should serialize");
        assert_eq!(result.is_error, Some(true));
        assert!(text["content"][0]["text"].as_str().is_some_and(|value| {
            value.contains("tldr tool failed: native-tldr daemon is unavailable for")
        }));
        assert_eq!(
            text["structuredContent"]["structuredFailure"]["error_type"],
            "daemon_unavailable"
        );
        assert_eq!(
            text["structuredContent"]["degradedMode"]["mode"],
            "unavailable"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn ensure_daemon_running_respects_disabled_auto_start_config() {
        let tempdir = tempdir().expect("tempdir should exist");
        let codex_dir = tempdir.path().join(".codex");
        std::fs::create_dir(&codex_dir).expect("config dir should exist");
        std::fs::write(
            codex_dir.join("tldr.toml"),
            "[daemon]\nauto_start = false\n",
        )
        .expect("config file should exist");

        let started = ensure_daemon_running(tempdir.path())
            .await
            .expect("ensure daemon should succeed");

        assert!(!started);
    }

    fn create_artifact_parent(path: &std::path::Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("artifact parent should be created");
        }
    }

    #[test]
    fn cleanup_stale_artifacts_removes_socket_and_pid_when_no_lock_is_held() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        std::fs::write(&socket_path, "").expect("socket path should be writable");
        std::fs::write(&pid_path, "999999").expect("pid path should be writable");

        cleanup_stale_artifacts(project_root);

        assert!(!socket_path.exists());
        assert!(!pid_path.exists());
    }

    #[test]
    fn cleanup_stale_artifacts_keeps_files_while_launcher_lock_is_held() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        let lock_path = codex_native_tldr::daemon::launch_lock_path_for_project(project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        create_artifact_parent(&lock_path);
        std::fs::write(&socket_path, "").expect("socket path should be writable");
        std::fs::write(&pid_path, "999999").expect("pid path should be writable");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .expect("launch lock file should open");
        lock_file
            .try_lock()
            .expect("launch lock should be acquired");

        cleanup_stale_artifacts(project_root);

        assert!(socket_path.exists());
        assert!(pid_path.exists());
    }

    #[test]
    fn daemon_metadata_looks_alive_with_launcher_lock_cleans_up_stale_files() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        std::fs::write(&socket_path, "").expect("socket path should be writable");
        std::fs::write(&pid_path, "999999").expect("pid path should be writable");

        let alive = daemon_metadata_looks_alive_with_launcher_lock(project_root, false);

        assert!(!alive);
        assert!(!socket_path.exists());
        assert!(!pid_path.exists());
    }

    #[test]
    fn daemon_metadata_looks_alive_with_launcher_lock_keeps_stale_files_while_locked() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path();
        let socket_path = socket_path_for_project(project_root);
        let pid_path = pid_path_for_project(project_root);
        let lock_path = codex_native_tldr::daemon::launch_lock_path_for_project(project_root);
        create_artifact_parent(&socket_path);
        create_artifact_parent(&pid_path);
        create_artifact_parent(&lock_path);
        std::fs::write(&socket_path, "").expect("socket path should be writable");
        std::fs::write(&pid_path, "999999").expect("pid path should be writable");
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .expect("launch lock file should open");
        lock_file
            .try_lock()
            .expect("launch lock should be acquired");

        let alive = daemon_metadata_looks_alive_with_launcher_lock(project_root, false);

        assert!(!alive);
        assert!(socket_path.exists());
        assert!(pid_path.exists());
    }

    #[test]
    fn try_open_launcher_lock_recovers_after_lock_file_is_deleted() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("deleted-launch-lock-project");
        let lock_path = codex_native_tldr::daemon::launch_lock_path_for_project(&project_root);
        create_artifact_parent(&lock_path);
        std::fs::write(&lock_path, "").expect("launch lock should exist before deletion");
        std::fs::remove_file(&lock_path).expect("launch lock should be removed");
        assert!(!lock_path.exists());

        let lock_is_held =
            launcher_lock_is_held(&project_root).expect("launcher lock query should succeed");

        assert!(!lock_is_held);
        assert!(lock_path.exists());
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_retries_when_external_daemon_becomes_ready() {
        let tempdir = tempdir().expect("tempdir should exist");
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));
        let query_response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            imports: None,
            importers: None,
            search: None,
            diagnostics: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let response = query_daemon_with_hooks(
            tempdir.path(),
            &command,
            &{
                let query_calls = Arc::clone(&query_calls);
                let query_response = query_response.clone();
                move |_project_root, _command| {
                    let query_calls = Arc::clone(&query_calls);
                    let query_response = query_response.clone();
                    Box::pin(async move {
                        let call_index = query_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(if call_index == 0 {
                            None
                        } else {
                            Some(query_response)
                        })
                    })
                }
            },
            &{
                let ensure_calls = Arc::clone(&ensure_calls);
                move |_project_root| {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    Box::pin(async move {
                        ensure_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(true)
                    })
                }
            },
        )
        .await
        .expect("query_daemon_with_hooks should succeed");

        assert_eq!(response, Some(query_response));
        assert_eq!(query_calls.load(Ordering::SeqCst), 2);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_skips_retry_when_no_external_daemon_is_starting() {
        let tempdir = tempdir().expect("tempdir should exist");
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));

        let response = query_daemon_with_hooks(
            tempdir.path(),
            &command,
            &{
                let query_calls = Arc::clone(&query_calls);
                move |_project_root, _command| {
                    let query_calls = Arc::clone(&query_calls);
                    Box::pin(async move {
                        query_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(None)
                    })
                }
            },
            &{
                let ensure_calls = Arc::clone(&ensure_calls);
                move |_project_root| {
                    let ensure_calls = Arc::clone(&ensure_calls);
                    Box::pin(async move {
                        ensure_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(false)
                    })
                }
            },
        )
        .await
        .expect("query_daemon_with_hooks should succeed");

        assert_eq!(response, None);
        assert_eq!(query_calls.load(Ordering::SeqCst), 1);
        assert_eq!(ensure_calls.load(Ordering::SeqCst), 1);
    }
}

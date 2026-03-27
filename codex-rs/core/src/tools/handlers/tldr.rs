use super::parse_arguments;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::registry::ToolHandler;
use crate::tools::registry::ToolKind;
use anyhow::Result;
use async_trait::async_trait;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::daemon_lock_is_held;
use codex_native_tldr::daemon::lock_path_for_project;
use codex_native_tldr::daemon::pid_path_for_project;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::daemon::socket_path_for_project;
use codex_native_tldr::tool_api::TldrToolCallParam;
use codex_native_tldr::tool_api::daemon_metadata_looks_alive;
use codex_native_tldr::tool_api::run_tldr_tool_with_hooks;
use std::ffi::OsString;
use std::fs::File;
use std::fs::OpenOptions;
use std::path::Path;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use std::time::Instant;
use tokio::process::Command;
use tokio::time::sleep;

pub struct TldrHandler;

const TLDR_JSON_BEGIN: &str = "---BEGIN_TLDR_JSON---";
const TLDR_JSON_END: &str = "---END_TLDR_JSON---";

#[async_trait]
impl ToolHandler for TldrHandler {
    type Output = FunctionToolOutput;

    fn kind(&self) -> ToolKind {
        ToolKind::Function
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation { turn, payload, .. } = invocation;
        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "tldr handler received unsupported payload".to_string(),
                ));
            }
        };
        let mut args: TldrToolCallParam = parse_arguments(&arguments)?;
        if args.project.is_none() {
            args.project = Some(turn.cwd.display().to_string());
        }

        run_tldr_handler_with_hooks(
            args,
            &|project_root, command| Box::pin(query_daemon(project_root, command)),
            &|project_root| Box::pin(ensure_daemon_running(project_root)),
        )
        .await
    }
}

async fn run_tldr_handler_with_hooks<Q, E>(
    args: TldrToolCallParam,
    query: &Q,
    ensure_running: &E,
) -> Result<FunctionToolOutput, FunctionCallError>
where
    Q: for<'a> Fn(
        &'a Path,
        &'a TldrDaemonCommand,
    ) -> codex_native_tldr::tool_api::QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> codex_native_tldr::tool_api::EnsureDaemonFuture<'a>,
{
    match run_tldr_tool_with_hooks(args, query, ensure_running).await {
        Ok(result) => {
            let json = serde_json::to_string_pretty(&result.structured_content)
                .map_err(|err| FunctionCallError::Fatal(format!("serialize tldr output: {err}")))?;
            let summary = render_tldr_summary(&result.structured_content);
            let rendered_text = sanitize_tldr_text(&result.text);
            Ok(FunctionToolOutput::from_text(
                format!("{rendered_text}\n{summary}\n{TLDR_JSON_BEGIN}\n{json}\n{TLDR_JSON_END}"),
                Some(true),
            ))
        }
        Err(err) => Ok(FunctionToolOutput::from_text(err.to_string(), Some(false))),
    }
}

fn sanitize_tldr_text(text: &str) -> String {
    text.replace(TLDR_JSON_BEGIN, "[BEGIN TLDR JSON]")
        .replace(TLDR_JSON_END, "[END TLDR JSON]")
}

fn render_tldr_summary(payload: &serde_json::Value) -> String {
    let mut parts = Vec::new();

    if let Some(kind) = payload
        .get("analysis")
        .and_then(|analysis| analysis.get("kind"))
        .and_then(serde_json::Value::as_str)
    {
        parts.push(format!("analysis kind: {kind}"));
    }

    if let Some(details) = payload
        .get("analysis")
        .and_then(|analysis| analysis.get("details"))
    {
        if let Some(node_count) = details
            .get("nodes")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("nodes: {node_count}"));
        }
        if let Some(edge_count) = details
            .get("edges")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("edges: {edge_count}"));
        }
        if let Some(symbol_count) = details
            .get("symbol_index")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("symbol index: {symbol_count}"));
        }
    }

    if parts.is_empty() && payload.get("semantic").is_some() {
        if let Some(query) = payload.get("query").and_then(serde_json::Value::as_str) {
            parts.push(format!("semantic query: {query}"));
        }
        if let Some(enabled) = payload.get("enabled").and_then(serde_json::Value::as_bool) {
            parts.push(format!("enabled: {enabled}"));
        }
        if let Some(match_count) = payload
            .get("matches")
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("matches: {match_count}"));
        }
    }

    if parts.is_empty() && payload.get("imports").is_some() {
        if let Some(path) = payload.get("path").and_then(serde_json::Value::as_str) {
            parts.push(format!("imports path: {path}"));
        }
        if let Some(import_count) = payload
            .get("imports")
            .and_then(|imports| imports.get("imports"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("imports: {import_count}"));
        }
    }

    if parts.is_empty() && payload.get("importers").is_some() {
        if let Some(module) = payload.get("module").and_then(serde_json::Value::as_str) {
            parts.push(format!("importers module: {module}"));
        }
        if let Some(match_count) = payload
            .get("importers")
            .and_then(|importers| importers.get("matches"))
            .and_then(serde_json::Value::as_array)
            .map(Vec::len)
        {
            parts.push(format!("matches: {match_count}"));
        }
    }

    if parts.is_empty()
        && (payload.get("status").is_some()
            || payload.get("snapshot").is_some()
            || payload.get("daemonStatus").is_some()
            || payload.get("reindexReport").is_some())
    {
        if let Some(action) = payload.get("action").and_then(serde_json::Value::as_str) {
            parts.push(format!("action: {action}"));
        }
        if let Some(status) = payload.get("status").and_then(serde_json::Value::as_str) {
            parts.push(format!("status: {status}"));
        }
        if let Some(message) = payload.get("message").and_then(serde_json::Value::as_str) {
            parts.push(format!("message: {message}"));
        }
    }

    if parts.is_empty() {
        "structured payload attached".to_string()
    } else {
        parts.join(" | ")
    }
}

async fn ensure_daemon_running(project_root: &Path) -> Result<bool> {
    if !cfg!(unix) {
        return Ok(false);
    }

    if daemon_metadata_looks_alive(project_root) {
        return Ok(true);
    }
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup(project_root).await;
    }

    let Some(launcher_lock) = try_open_launcher_lock(project_root)? else {
        return wait_for_daemon_startup(project_root).await;
    };

    if daemon_metadata_looks_alive(project_root) {
        return Ok(true);
    }
    if daemon_lock_is_held(project_root)? {
        return wait_for_daemon_startup_during_launch(project_root).await;
    }

    cleanup_stale_artifacts(project_root);

    let mut child = daemon_launcher_command(project_root)?
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;

    tokio::spawn(async move {
        let _ = child.wait().await;
    });

    let started = wait_for_daemon_startup_during_launch(project_root).await;
    drop(launcher_lock);
    started
}

fn daemon_launcher_command(project_root: &Path) -> Result<Command> {
    let current_exe = std::env::current_exe()?;
    let mut command = Command::new(current_exe);
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

async fn wait_for_daemon_startup(project_root: &Path) -> Result<bool> {
    wait_for_daemon_startup_with_launcher_lock(project_root, false).await
}

async fn wait_for_daemon_startup_during_launch(project_root: &Path) -> Result<bool> {
    wait_for_daemon_startup_with_launcher_lock(project_root, true).await
}

async fn wait_for_daemon_startup_with_launcher_lock(
    project_root: &Path,
    ignore_launcher_lock: bool,
) -> Result<bool> {
    let start = Instant::now();
    let timeout = Duration::from_secs(3);
    while start.elapsed() < timeout {
        if daemon_metadata_looks_alive_with_launcher_lock(project_root, ignore_launcher_lock) {
            return Ok(true);
        }
        sleep(Duration::from_millis(50)).await;
    }
    Ok(false)
}

fn daemon_metadata_looks_alive_with_launcher_lock(
    project_root: &Path,
    ignore_launcher_lock: bool,
) -> bool {
    match codex_native_tldr::daemon::daemon_health(project_root) {
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

fn cleanup_stale_artifacts(project_root: &Path) {
    if launcher_lock_is_held(project_root).unwrap_or(false) {
        return;
    }

    let Ok(health) = codex_native_tldr::daemon::daemon_health(project_root) else {
        return;
    };
    if !health.should_cleanup_artifacts() {
        return;
    }
    cleanup_file_if_exists(socket_path_for_project(project_root));
    cleanup_file_if_exists(pid_path_for_project(project_root));
}

fn launcher_lock_path_for_project(project_root: &Path) -> PathBuf {
    lock_path_for_project(project_root).with_extension("launch.lock")
}

fn try_open_launcher_lock(project_root: &Path) -> Result<Option<File>> {
    let lock_path = launcher_lock_path_for_project(project_root);
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

#[cfg(test)]
mod helper_tests {
    use super::cleanup_file_if_exists;
    use super::daemon_launcher_args;
    use super::launcher_lock_path_for_project;
    use pretty_assertions::assert_eq;
    use std::ffi::OsString;
    use tempfile::tempdir;

    #[test]
    fn daemon_launcher_args_use_internal_daemon_entrypoint() {
        let project_root = std::path::Path::new("/tmp/project-root");
        let args = daemon_launcher_args(project_root);
        assert_eq!(
            args,
            [
                OsString::from("tldr"),
                OsString::from("internal-daemon"),
                OsString::from("--project"),
                OsString::from("/tmp/project-root"),
            ]
        );
    }

    #[test]
    fn launcher_lock_path_uses_launch_lock_extension() {
        let project_root = std::path::Path::new("/tmp/project-root");
        let lock_path = launcher_lock_path_for_project(project_root);
        assert!(lock_path.to_string_lossy().ends_with(".launch.lock"));
    }

    #[test]
    fn cleanup_file_if_exists_removes_existing_file() {
        let tempdir = tempdir().expect("tempdir should exist");
        let file_path = tempdir.path().join("stale.sock");
        std::fs::write(&file_path, "stale").expect("fixture should write");

        cleanup_file_if_exists(file_path.clone());

        assert_eq!(file_path.exists(), false);
    }

    #[test]
    fn cleanup_file_if_exists_ignores_missing_file() {
        let tempdir = tempdir().expect("tempdir should exist");
        let missing = tempdir.path().join("missing.sock");
        cleanup_file_if_exists(missing.clone());
        assert_eq!(missing.exists(), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codex::make_session_and_context;
    use crate::turn_diff_tracker::TurnDiffTracker;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::tempdir;
    use tokio::sync::Mutex;

    fn invocation(
        session: Arc<crate::codex::Session>,
        turn: Arc<crate::codex::TurnContext>,
        arguments: serde_json::Value,
    ) -> ToolInvocation {
        ToolInvocation {
            session,
            turn,
            tracker: Arc::new(Mutex::new(TurnDiffTracker::default())),
            call_id: "call-1".to_string(),
            tool_name: "tldr".to_string(),
            tool_namespace: None,
            payload: ToolPayload::Function {
                arguments: arguments.to_string(),
            },
        }
    }

    fn daemon_ok(message: &str) -> TldrDaemonResponse {
        TldrDaemonResponse {
            status: "ok".to_string(),
            message: message.to_string(),
            analysis: None,
            imports: None,
            importers: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        }
    }

    fn extract_json_block(text: &str) -> serde_json::Value {
        let (prefix, json_and_suffix) = text
            .split_once(&format!("\n{TLDR_JSON_BEGIN}\n"))
            .expect("tldr output should include a begin marker on its own line");
        assert!(
            !prefix.is_empty(),
            "tldr output should preserve the rendered text before the JSON block"
        );

        let json = json_and_suffix
            .strip_suffix(&format!("\n{TLDR_JSON_END}"))
            .expect("tldr output should end with the closing marker");
        serde_json::from_str(json).expect("json block should parse")
    }

    #[tokio::test]
    async fn handler_defaults_project_to_turn_cwd_and_falls_back_to_local_engine() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src_dir = tempdir.path().join("src");
        std::fs::create_dir_all(&src_dir).expect("src dir should exist");
        std::fs::write(
            src_dir.join("lib.rs"),
            "pub struct AuthService;\nimpl AuthService { pub fn login(&self) {} }\n",
        )
        .expect("source should write");

        let (session, mut turn) = make_session_and_context().await;
        turn.cwd = tempdir.path().to_path_buf();
        let output = TldrHandler
            .handle(invocation(
                Arc::new(session),
                Arc::new(turn),
                json!({
                    "action": "tree",
                    "language": "rust",
                    "symbol": "AuthService"
                }),
            ))
            .await
            .expect("handler should succeed");
        let text = output.into_text();

        assert!(text.contains("tree rust via local"));
        assert!(text.contains("\"project\":"));
        assert!(text.contains(tempdir.path().to_string_lossy().as_ref()));
        assert!(text.contains("\"symbol\": \"AuthService\""));
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_formats_daemon_semantic_payload() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join(".codex")).expect("config dir should exist");

        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Semantic,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: None,
                query: Some("auth login".to_string()),
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        semantic: Some(codex_native_tldr::semantic::SemanticSearchResponse {
                            enabled: true,
                            query: "auth login".to_string(),
                            indexed_files: 1,
                            truncated: false,
                            matches: vec![codex_native_tldr::semantic::SemanticMatch {
                                score: 10,
                                path: PathBuf::from("src/lib.rs"),
                                line: 1,
                                snippet: "pub struct AuthService;".to_string(),
                                unit: codex_native_tldr::semantic::EmbeddingUnit {
                                    path: PathBuf::from("src/lib.rs"),
                                    language:
                                        codex_native_tldr::lang_support::SupportedLanguage::Rust,
                                    symbol: Some("AuthService".to_string()),
                                    qualified_symbol: Some("auth::AuthService".to_string()),
                                    symbol_aliases: vec!["AuthService".to_string()],
                                    kind: "struct".to_string(),
                                    line: 1,
                                    span_end_line: 1,
                                    module_path: vec!["auth".to_string()],
                                    visibility: Some("pub".to_string()),
                                    signature: Some("pub struct AuthService".to_string()),
                                    docs: Vec::new(),
                                    imports: Vec::new(),
                                    references: Vec::new(),
                                    code_preview: "pub struct AuthService;".to_string(),
                                    calls: Vec::new(),
                                    called_by: Vec::new(),
                                    dependencies: Vec::new(),
                                    cfg_summary: "cfg".to_string(),
                                    dfg_summary: "dfg".to_string(),
                                    embedding_vector: None,
                                },
                                embedding_text: "internal".to_string(),
                                embedding_score: Some(0.75),
                            }],
                            embedding_used: true,
                            message: "semantic search returned 1 matches".to_string(),
                        }),
                        ..daemon_ok("semantic")
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("semantic rust enabled=true via daemon"));
        assert!(text.contains("semantic query: auth login | enabled: true | matches: 1"));
        assert!(text.contains(TLDR_JSON_BEGIN));
        assert!(text.contains("\"qualifiedSymbol\": \"auth::AuthService\""));
        assert!(text.contains("\"kind\": \"struct\""));
        assert!(text.contains("\"signature\": \"pub struct AuthService\""));
        assert!(text.contains("\"embedding_score\": 0.75"));
        assert!(text.contains(TLDR_JSON_END));
        assert_eq!(payload["action"], "semantic");
        assert_eq!(payload["semantic"]["query"], "auth login");
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_returns_error_text_for_invalid_semantic_request() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Semantic,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| Box::pin(async move { Ok(None) }),
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should return tool output");

        assert_eq!(output.success, Some(false));
        assert!(
            output
                .into_text()
                .contains("`query` is required for action=semantic")
        );
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_formats_analysis_graph_details() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Tree,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: Some("AuthService".to_string()),
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Ast,
                            summary: "structure summary".to_string(),
                            details: Some(codex_native_tldr::api::AnalysisDetail {
                                indexed_files: 1,
                                total_symbols: 1,
                                symbol_query: Some("AuthService".to_string()),
                                truncated: false,
                                change_paths: Vec::new(),
                                slice_target: None,
                                slice_lines: Vec::new(),
                                overview: codex_native_tldr::api::AnalysisOverviewDetail {
                                    kinds: vec![codex_native_tldr::api::AnalysisCountDetail {
                                        name: "struct".to_string(),
                                        count: 1,
                                    }],
                                    outgoing_edges: 1,
                                    incoming_edges: 0,
                                    reference_count: 0,
                                    import_count: 0,
                                },
                                files: vec![codex_native_tldr::api::AnalysisFileDetail {
                                    path: "src/lib.rs".to_string(),
                                    symbol_count: 1,
                                    kinds: vec![codex_native_tldr::api::AnalysisCountDetail {
                                        name: "struct".to_string(),
                                        count: 1,
                                    }],
                                }],
                                nodes: vec![codex_native_tldr::api::AnalysisNodeDetail {
                                    id: "AuthService".to_string(),
                                    label: "AuthService".to_string(),
                                    kind: "struct".to_string(),
                                    path: Some("src/lib.rs".to_string()),
                                    line: Some(1),
                                    signature: Some("pub struct AuthService".to_string()),
                                }],
                                edges: vec![codex_native_tldr::api::AnalysisEdgeDetail {
                                    from: "AuthService".to_string(),
                                    to: "validate".to_string(),
                                    kind: "calls".to_string(),
                                }],
                                symbol_index: vec![
                                    codex_native_tldr::api::AnalysisSymbolIndexEntry {
                                        symbol: "AuthService".to_string(),
                                        node_ids: vec!["AuthService".to_string()],
                                    },
                                ],
                                units: vec![codex_native_tldr::api::AnalysisUnitDetail {
                                    path: "src/lib.rs".to_string(),
                                    line: 1,
                                    span_end_line: 1,
                                    symbol: Some("AuthService".to_string()),
                                    qualified_symbol: None,
                                    kind: "struct".to_string(),
                                    module_path: Vec::new(),
                                    visibility: Some("pub".to_string()),
                                    signature: Some("pub struct AuthService".to_string()),
                                    calls: vec!["validate".to_string()],
                                    called_by: Vec::new(),
                                    references: Vec::new(),
                                    imports: Vec::new(),
                                    dependencies: Vec::new(),
                                    cfg_summary: "cfg".to_string(),
                                    dfg_summary: "dfg".to_string(),
                                }],
                            }),
                        }),
                        ..daemon_ok("analysis")
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("analysis kind: ast"));
        assert!(text.contains("nodes: 1"));
        assert!(text.contains("edges: 1"));
        assert!(text.contains("symbol index: 1"));
        assert!(text.contains("\nanalysis kind: ast | nodes: 1 | edges: 1 | symbol index: 1\n"));
        assert_eq!(payload["action"], "tree");
        assert_eq!(payload["analysis"]["summary"], "structure summary");
        assert_eq!(
            payload["analysis"]["details"]["symbol_query"],
            "AuthService"
        );
        assert_eq!(
            payload["analysis"]["details"]["nodes"][0]["id"],
            "AuthService"
        );
        assert_eq!(payload["analysis"]["details"]["edges"][0]["kind"], "calls");
        assert_eq!(
            payload["analysis"]["details"]["symbol_index"][0]["symbol"],
            "AuthService"
        );
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_formats_cfg_analysis_details() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Cfg,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: Some("AuthService".to_string()),
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Cfg,
                            summary: "cfg summary".to_string(),
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
                                units: vec![codex_native_tldr::api::AnalysisUnitDetail {
                                    path: "src/lib.rs".to_string(),
                                    line: 1,
                                    span_end_line: 1,
                                    symbol: Some("AuthService".to_string()),
                                    qualified_symbol: Some("auth::AuthService".to_string()),
                                    kind: "struct".to_string(),
                                    module_path: Vec::new(),
                                    visibility: Some("pub".to_string()),
                                    signature: Some("pub struct AuthService".to_string()),
                                    calls: Vec::new(),
                                    called_by: Vec::new(),
                                    references: Vec::new(),
                                    imports: Vec::new(),
                                    dependencies: Vec::new(),
                                    cfg_summary: "cfg".to_string(),
                                    dfg_summary: "dfg".to_string(),
                                }],
                            }),
                        }),
                        ..daemon_ok("cfg")
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("analysis kind: cfg"));
        assert_eq!(payload["action"], "cfg");
        assert_eq!(payload["analysis"]["kind"], "cfg");
        assert_eq!(
            payload["analysis"]["details"]["symbol_query"],
            "AuthService"
        );
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_formats_change_impact_analysis_details() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::ChangeImpact,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: Some(vec!["src/lib.rs".to_string()]),
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::ChangeImpact,
                            summary:
                                "change-impact summary: 1 changed paths -> 2 impacted symbols across 1 indexed files"
                                    .to_string(),
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
                        ..daemon_ok("change-impact")
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("change-impact rust via daemon"));
        assert!(text.contains("analysis kind: change_impact"));
        assert_eq!(payload["action"], "change-impact");
        assert_eq!(payload["paths"], serde_json::json!(["src/lib.rs"]));
        assert_eq!(payload["analysis"]["kind"], "change_impact");
        assert_eq!(
            payload["analysis"]["details"]["change_paths"],
            serde_json::json!(["src/lib.rs"])
        );
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_formats_extract_analysis_details() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Extract,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Extract,
                            summary:
                                "extract summary: src/lib.rs => 1 symbols (1 function); imports=0, references=0; sample: main:1-1"
                                    .to_string(),
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
                                units: vec![codex_native_tldr::api::AnalysisUnitDetail {
                                    path: "src/lib.rs".to_string(),
                                    line: 1,
                                    span_end_line: 1,
                                    symbol: Some("main".to_string()),
                                    qualified_symbol: None,
                                    kind: "function".to_string(),
                                    module_path: vec!["crate".to_string()],
                                    visibility: None,
                                    signature: Some("fn main()".to_string()),
                                    calls: Vec::new(),
                                    called_by: Vec::new(),
                                    references: Vec::new(),
                                    imports: Vec::new(),
                                    dependencies: Vec::new(),
                                    cfg_summary: "cfg".to_string(),
                                    dfg_summary: "dfg".to_string(),
                                }],
                            }),
                        }),
                        ..daemon_ok("extract")
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("extract rust via daemon"));
        assert!(text.contains("analysis kind: extract"));
        assert_eq!(payload["action"], "extract");
        assert_eq!(payload["path"], "src/lib.rs");
        assert_eq!(payload["analysis"]["kind"], "extract");
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_formats_slice_analysis_details() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Slice,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: Some("login".to_string()),
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: Some(4),
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Slice,
                            summary:
                                "slice summary: backward slice for src/lib.rs:login:4 -> 3 lines [1, 3, 4]"
                                    .to_string(),
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
                        ..daemon_ok("slice")
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("analysis kind: slice"));
        assert_eq!(payload["action"], "slice");
        assert_eq!(payload["line"], 4);
        assert_eq!(payload["analysis"]["kind"], "slice");
        assert_eq!(
            payload["analysis"]["details"]["slice_lines"],
            serde_json::json!([1, 3, 4])
        );
    }

    #[test]
    fn render_tldr_summary_falls_back_without_analysis_details() {
        let payload = serde_json::json!({
            "semantic": {
                "enabled": true
            }
        });

        assert_eq!(render_tldr_summary(&payload), "structured payload attached");
    }

    #[test]
    fn render_tldr_summary_surfaces_semantic_payload_fields() {
        let payload = serde_json::json!({
            "action": "semantic",
            "query": "auth login",
            "enabled": true,
            "matches": [{}],
            "semantic": {
                "query": "auth login"
            }
        });

        assert_eq!(
            render_tldr_summary(&payload),
            "semantic query: auth login | enabled: true | matches: 1"
        );
    }

    #[test]
    fn render_tldr_summary_surfaces_imports_payload_fields() {
        let payload = serde_json::json!({
            "action": "imports",
            "path": "src/lib.rs",
            "imports": {
                "imports": ["use crate::auth::token;"]
            }
        });

        assert_eq!(
            render_tldr_summary(&payload),
            "imports path: src/lib.rs | imports: 1"
        );
    }

    #[test]
    fn render_tldr_summary_surfaces_importers_payload_fields() {
        let payload = serde_json::json!({
            "action": "importers",
            "module": "auth::token",
            "importers": {
                "matches": [{"path": "src/lib.rs"}]
            }
        });

        assert_eq!(
            render_tldr_summary(&payload),
            "importers module: auth::token | matches: 1"
        );
    }

    #[test]
    fn render_tldr_summary_surfaces_daemon_payload_fields() {
        let payload = serde_json::json!({
            "action": "status",
            "status": "ok",
            "message": "status"
        });

        assert_eq!(
            render_tldr_summary(&payload),
            "action: status | status: ok | message: status"
        );
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_summarizes_ping_payload() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Ping,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| Box::pin(async move { Ok(Some(daemon_ok("pong"))) }),
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("action: ping | status: ok | message: pong"));
        assert_eq!(payload["action"], "ping");
        assert_eq!(payload["message"], "pong");
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_summarizes_notify_payload() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Notify,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: Some("src/lib.rs".to_string()),
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "marked src/lib.rs dirty".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        semantic: None,
                        snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                            cached_entries: 1,
                            dirty_files: 1,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            last_query_at: None,
                            last_reindex: None,
                            last_reindex_attempt: None,
                        }),
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("action: notify | status: ok | message: marked src/lib.rs dirty"));
        assert_eq!(payload["action"], "notify");
        assert_eq!(payload["snapshot"]["dirty_files"], 1);
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_summarizes_snapshot_payload() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Snapshot,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "snapshot".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        semantic: None,
                        snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                            cached_entries: 2,
                            dirty_files: 1,
                            dirty_file_threshold: 20,
                            reindex_pending: true,
                            last_query_at: None,
                            last_reindex: None,
                            last_reindex_attempt: None,
                        }),
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("action: snapshot | status: ok | message: snapshot"));
        assert_eq!(payload["action"], "snapshot");
        assert_eq!(payload["snapshot"]["cached_entries"], 2);
        assert_eq!(payload["snapshot"]["dirty_files"], 1);
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_preserves_status_detail_fields() {
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
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Status,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                let report = report.clone();
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "status".to_string(),
                        analysis: None,
                        imports: None,
                        importers: None,
                        semantic: None,
                        snapshot: Some(codex_native_tldr::session::SessionSnapshot {
                            cached_entries: 1,
                            dirty_files: 0,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                            last_reindex: Some(report.clone()),
                            last_reindex_attempt: Some(report.clone()),
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
                            last_query_at: Some(std::time::SystemTime::UNIX_EPOCH),
                            config: codex_native_tldr::daemon::TldrDaemonConfigSummary {
                                auto_start: true,
                                socket_mode: "unix".to_string(),
                                semantic_enabled: true,
                                semantic_auto_reindex_threshold: 20,
                                session_dirty_file_threshold: 20,
                            },
                        }),
                        reindex_report: Some(report),
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let payload = extract_json_block(&text);

        assert!(text.contains("action: status | status: ok | message: status"));
        assert_eq!(payload["daemonStatus"]["healthy"], true);
        assert_eq!(payload["reindexReport"]["status"], "Completed");
        assert_eq!(payload["snapshot"]["last_reindex"]["status"], "Completed");
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_returns_error_text_for_missing_language() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Tree,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| Box::pin(async move { Ok(None) }),
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should return tool output");

        assert_eq!(output.success, Some(false));
        assert_eq!(output.into_text(), "`language` is required for action=tree");
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_returns_error_text_when_daemon_is_unavailable() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Status,
                project: Some(tempdir.path().display().to_string()),
                language: None,
                symbol: None,
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| Box::pin(async move { Ok(None) }),
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should return tool output");

        assert_eq!(output.success, Some(false));
        assert!(
            output
                .into_text()
                .contains("native-tldr daemon is unavailable for")
        );
    }

    #[tokio::test]
    async fn run_tldr_handler_with_hooks_sanitizes_marker_collisions_in_text() {
        let tempdir = tempdir().expect("tempdir should exist");
        let output = run_tldr_handler_with_hooks(
            TldrToolCallParam {
                action: codex_native_tldr::tool_api::TldrToolAction::Tree,
                project: Some(tempdir.path().display().to_string()),
                language: Some(codex_native_tldr::tool_api::TldrToolLanguage::Rust),
                symbol: Some("AuthService".to_string()),
                query: None,
                module: None,
                path: None,
                line: None,
                paths: None,
            },
            &|_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        analysis: Some(codex_native_tldr::api::AnalysisResponse {
                            kind: codex_native_tldr::api::AnalysisKind::Ast,
                            summary: "structure ---BEGIN_TLDR_JSON--- summary ---END_TLDR_JSON---"
                                .to_string(),
                            details: None,
                        }),
                        status: "ok".to_string(),
                        message: "analysis".to_string(),
                        imports: None,
                        importers: None,
                        semantic: None,
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            &|_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("handler helper should succeed");
        let text = output.into_text();
        let (prefix, _) = text
            .split_once(&format!("\n{TLDR_JSON_BEGIN}\n"))
            .expect("tldr output should include a begin marker");

        assert!(prefix.contains("[BEGIN TLDR JSON]"));
        assert!(prefix.contains("[END TLDR JSON]"));
        let payload = extract_json_block(&text);
        assert_eq!(
            payload["analysis"]["summary"],
            "structure ---BEGIN_TLDR_JSON--- summary ---END_TLDR_JSON---"
        );
    }
}

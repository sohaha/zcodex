use anyhow::Result;
use codex_native_tldr::TldrEngine;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisRequest;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::daemon_health;
use codex_native_tldr::daemon::daemon_lock_is_held;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::lang_support::LanguageRegistry;
use codex_native_tldr::lang_support::SupportedLanguage;
use codex_native_tldr::lifecycle::DaemonLifecycleManager;
use codex_native_tldr::load_tldr_config;
use codex_native_tldr::semantic::SemanticSearchRequest;
use codex_native_tldr::semantic::SemanticSearchResponse;
use codex_native_tldr::wire::daemon_response_payload;
use codex_native_tldr::wire::semantic_payload;
use once_cell::sync::Lazy;
use rmcp::model::CallToolResult;
use rmcp::model::Content;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use schemars::JsonSchema;
use schemars::r#gen::SchemaSettings;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TldrToolAction {
    Tree,
    Context,
    Impact,
    Semantic,
    Ping,
    Warm,
    Snapshot,
    Status,
    Notify,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TldrToolLanguage {
    Rust,
    Typescript,
    Javascript,
    Python,
    Go,
    Php,
    Zig,
}

impl From<TldrToolLanguage> for SupportedLanguage {
    fn from(value: TldrToolLanguage) -> Self {
        match value {
            TldrToolLanguage::Rust => SupportedLanguage::Rust,
            TldrToolLanguage::Typescript => SupportedLanguage::TypeScript,
            TldrToolLanguage::Javascript => SupportedLanguage::JavaScript,
            TldrToolLanguage::Python => SupportedLanguage::Python,
            TldrToolLanguage::Go => SupportedLanguage::Go,
            TldrToolLanguage::Php => SupportedLanguage::Php,
            TldrToolLanguage::Zig => SupportedLanguage::Zig,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TldrToolCallParam {
    pub action: TldrToolAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub language: Option<TldrToolLanguage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symbol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

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
        output_schema: Some(tldr_tool_output_schema()),
        annotations: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

type QueryDaemonFuture<'a> = Pin<
    Box<
        dyn Future<Output = Result<Option<codex_native_tldr::daemon::TldrDaemonResponse>>>
            + Send
            + 'a,
    >,
>;
type EnsureDaemonFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;

static DAEMON_LIFECYCLE_MANAGER: Lazy<DaemonLifecycleManager> =
    Lazy::new(DaemonLifecycleManager::default);

pub(crate) async fn run_tldr_tool(arguments: Option<JsonObject>) -> CallToolResult {
    let args = match arguments.map(serde_json::Value::Object) {
        Some(json_val) => match serde_json::from_value::<TldrToolCallParam>(json_val) {
            Ok(args) => args,
            Err(err) => return error_result(format!("Failed to parse tldr tool arguments: {err}")),
        },
        None => return error_result("Missing arguments for tldr tool-call.".to_string()),
    };

    match run_tldr_tool_inner(args).await {
        Ok(result) => result,
        Err(err) => error_result(format!("tldr tool failed: {err}")),
    }
}

async fn run_tldr_tool_inner(args: TldrToolCallParam) -> Result<CallToolResult> {
    let project_root = resolve_project_root(args.project.as_deref())?;
    match args.action {
        TldrToolAction::Tree => {
            let language = required_language(&args)?;
            run_analysis_tool(project_root, TldrToolAction::Tree, language, args.symbol).await
        }
        TldrToolAction::Context => {
            let language = required_language(&args)?;
            run_analysis_tool(project_root, TldrToolAction::Context, language, args.symbol).await
        }
        TldrToolAction::Impact => {
            let language = required_language(&args)?;
            run_analysis_tool(project_root, TldrToolAction::Impact, language, args.symbol).await
        }
        TldrToolAction::Semantic => run_semantic_tool(project_root, args).await,
        TldrToolAction::Ping => {
            run_daemon_tool(project_root, TldrDaemonCommand::Ping, "ping").await
        }
        TldrToolAction::Warm => {
            run_daemon_tool(project_root, TldrDaemonCommand::Warm, "warm").await
        }
        TldrToolAction::Snapshot => {
            run_daemon_tool(project_root, TldrDaemonCommand::Snapshot, "snapshot").await
        }
        TldrToolAction::Status => {
            run_daemon_tool(project_root, TldrDaemonCommand::Status, "status").await
        }
        TldrToolAction::Notify => {
            let path = args
                .path
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("`path` is required for action=notify"))?;
            run_daemon_tool(project_root, TldrDaemonCommand::Notify { path }, "notify").await
        }
    }
}

async fn run_analysis_tool(
    project_root: PathBuf,
    action: TldrToolAction,
    language: SupportedLanguage,
    symbol: Option<String>,
) -> Result<CallToolResult> {
    let kind = match action {
        TldrToolAction::Tree => AnalysisKind::Ast,
        TldrToolAction::Context => AnalysisKind::CallGraph,
        TldrToolAction::Impact => AnalysisKind::Pdg,
        _ => unreachable!("analysis action must map to AnalysisKind"),
    };
    let request = AnalysisRequest {
        kind,
        symbol: symbol.clone(),
    };
    let daemon_response = query_daemon_with_lifecycle(
        &project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(kind, language, symbol.as_deref()),
            request: request.clone(),
        },
    )
    .await?;
    let support = LanguageRegistry::support_for(language);
    let (source, message, summary) = if let Some(response) = daemon_response {
        let analysis = response
            .analysis
            .ok_or_else(|| anyhow::anyhow!("daemon response missing analysis payload"))?;
        ("daemon", response.message, analysis.summary)
    } else {
        let config = load_tldr_config(&project_root)?;
        let engine = TldrEngine::builder(project_root.clone())
            .with_config(config)
            .build();
        let response = engine.analyze(request)?;
        (
            "local",
            "daemon unavailable; used local engine".to_string(),
            response.summary,
        )
    };

    let structured_content = json!({
        "action": action_name(&action),
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "supportLevel": format!("{:?}", support.support_level),
        "fallbackStrategy": support.fallback_strategy,
        "summary": summary,
        "symbol": symbol,
    });
    let text = format!(
        "{} {} via {source}: {summary}",
        action_name(&action),
        language.as_str()
    );
    Ok(success_result(text, structured_content))
}

async fn run_semantic_tool(
    project_root: PathBuf,
    args: TldrToolCallParam,
) -> Result<CallToolResult> {
    let language = required_language(&args)?;
    let query = args
        .query
        .ok_or_else(|| anyhow::anyhow!("`query` is required for action=semantic"))?;
    let request = SemanticSearchRequest {
        language,
        query: query.clone(),
    };
    let daemon_response = query_daemon_with_lifecycle(
        &project_root,
        &TldrDaemonCommand::Semantic {
            request: request.clone(),
        },
    )
    .await?;
    let (response, source) = if let Some(response) = daemon_response {
        if let Some(semantic) = response.semantic {
            (semantic, "daemon")
        } else {
            (
                run_local_semantic(&project_root, language, &query)?,
                "local",
            )
        }
    } else {
        (
            run_local_semantic(&project_root, language, &query)?,
            "local",
        )
    };
    let structured_content =
        semantic_payload(Some("semantic"), &project_root, language, source, &response);
    Ok(success_result(
        format!(
            "semantic {} enabled={} via {source}: {}",
            language.as_str(),
            response.enabled,
            response.message
        ),
        structured_content,
    ))
}

fn run_local_semantic(
    project_root: &Path,
    language: SupportedLanguage,
    query: &str,
) -> Result<SemanticSearchResponse> {
    let config = load_tldr_config(project_root)?;
    let engine = TldrEngine::builder(project_root.to_path_buf())
        .with_config(config)
        .build();
    engine.semantic_search(SemanticSearchRequest {
        language,
        query: query.to_string(),
    })
}

async fn run_daemon_tool(
    project_root: PathBuf,
    command: TldrDaemonCommand,
    action: &str,
) -> Result<CallToolResult> {
    let Some(response) = query_daemon_with_lifecycle(&project_root, &command).await? else {
        return Err(anyhow::anyhow!(
            "native-tldr daemon is unavailable for {}",
            project_root.display()
        ));
    };
    let structured_content = daemon_response_payload(action, &project_root, &response);
    let text = structured_content
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ok")
        .to_string();
    Ok(success_result(text, structured_content))
}

async fn query_daemon_with_lifecycle(
    project_root: &std::path::Path,
    command: &TldrDaemonCommand,
) -> Result<Option<codex_native_tldr::daemon::TldrDaemonResponse>> {
    query_daemon_with_hooks(
        project_root,
        command,
        |project_root, command| Box::pin(query_daemon(project_root, command)),
        |project_root| Box::pin(wait_for_external_daemon(project_root)),
    )
    .await
}

async fn query_daemon_with_hooks<Q, E>(
    project_root: &std::path::Path,
    command: &TldrDaemonCommand,
    query: Q,
    ensure_running: E,
) -> Result<Option<codex_native_tldr::daemon::TldrDaemonResponse>>
where
    Q: for<'a> Fn(&'a std::path::Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a std::path::Path) -> EnsureDaemonFuture<'a>,
{
    DAEMON_LIFECYCLE_MANAGER
        .query_or_spawn_with_hooks(project_root, command, query, ensure_running)
        .await
}

async fn wait_for_external_daemon(project_root: &std::path::Path) -> Result<bool> {
    if !daemon_lock_is_held(project_root)? {
        return Ok(false);
    }

    let start = Instant::now();
    let timeout = Duration::from_secs(3);
    while start.elapsed() < timeout {
        if daemon_metadata_looks_alive(project_root) {
            return Ok(true);
        }
        sleep(Duration::from_millis(50)).await;
    }

    Ok(false)
}

fn daemon_metadata_looks_alive(project_root: &std::path::Path) -> bool {
    daemon_health(project_root)
        .map(|health| health.healthy)
        .unwrap_or(false)
}

fn required_language(args: &TldrToolCallParam) -> Result<SupportedLanguage> {
    let language = args.language.ok_or_else(|| {
        anyhow::anyhow!(
            "`language` is required for action={}",
            action_name(&args.action)
        )
    })?;
    Ok(language.into())
}

fn resolve_project_root(project: Option<&str>) -> Result<PathBuf> {
    let project_root = match project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir()?,
    };
    Ok(project_root.canonicalize()?)
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
    CallToolResult {
        content: vec![Content::text(text)],
        structured_content: None,
        is_error: Some(true),
        meta: None,
    }
}

fn action_name(action: &TldrToolAction) -> &'static str {
    match action {
        TldrToolAction::Tree => "tree",
        TldrToolAction::Context => "context",
        TldrToolAction::Impact => "impact",
        TldrToolAction::Semantic => "semantic",
        TldrToolAction::Ping => "ping",
        TldrToolAction::Warm => "warm",
        TldrToolAction::Snapshot => "snapshot",
        TldrToolAction::Status => "status",
        TldrToolAction::Notify => "notify",
    }
}

fn analysis_cache_key(
    kind: AnalysisKind,
    language: SupportedLanguage,
    symbol: Option<&str>,
) -> String {
    let symbol = symbol.unwrap_or("*");
    format!("{}:{kind:?}:{symbol}", language.as_str())
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

fn tldr_tool_output_schema() -> Arc<JsonObject> {
    let schema = json!({
        "type": "object",
        "properties": {
            "action": { "type": "string" },
            "project": { "type": "string" },
            "source": { "type": "string" },
            "status": { "type": "string" },
            "message": { "type": "string" },
            "summary": { "type": "string" },
            "daemonStatus": { "type": "object" }
        },
    });
    match schema {
        serde_json::Value::Object(map) => Arc::new(map),
        _ => unreachable!("json literal must be an object"),
    }
}

#[cfg(test)]
mod tests {
    use super::TldrToolAction;
    use super::TldrToolCallParam;
    use super::TldrToolLanguage;
    use super::create_tool_for_tldr_tool_call_param;
    use super::query_daemon_with_hooks;
    use codex_native_tldr::daemon::TldrDaemonCommand;
    use codex_native_tldr::daemon::TldrDaemonResponse;
    use pretty_assertions::assert_eq;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use tempfile::tempdir;

    #[test]
    fn verify_tldr_tool_json_schema() {
        let tool = create_tool_for_tldr_tool_call_param();
        let tool_json = serde_json::to_value(&tool).expect("tool serializes");
        let expected_tool_json = serde_json::json!({
          "description": "Structured code context analysis via native-tldr with daemon-first execution.",
          "inputSchema": {
            "properties": {
              "action": {
                "enum": ["tree", "context", "impact", "semantic", "ping", "warm", "snapshot", "status", "notify"],
                "type": "string"
              },
              "language": {
                "enum": ["rust", "typescript", "javascript", "python", "go", "php", "zig"],
                "type": "string"
              },
              "path": { "type": "string" },
              "project": { "type": "string" },
              "query": { "type": "string" },
              "symbol": { "type": "string" }
            },
            "required": ["action"],
            "type": "object"
          },
          "name": "tldr",
          "outputSchema": {
            "properties": {
              "action": { "type": "string" },
              "daemonStatus": { "type": "object" },
              "message": { "type": "string" },
              "project": { "type": "string" },
              "source": { "type": "string" },
              "status": { "type": "string" },
              "summary": { "type": "string" }
            },
            "type": "object"
          },
          "title": "Native TLDR"
        });

        assert_eq!(expected_tool_json, tool_json);
    }

    #[test]
    fn tldr_tool_param_serializes_camel_case_fields() {
        let value = serde_json::to_value(TldrToolCallParam {
            action: TldrToolAction::Semantic,
            project: Some("/tmp/project".to_string()),
            language: Some(TldrToolLanguage::Typescript),
            symbol: None,
            query: Some("where is auth".to_string()),
            path: None,
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
    async fn query_daemon_with_hooks_retries_when_external_daemon_becomes_ready() {
        let tempdir = tempdir().expect("tempdir should exist");
        let command = TldrDaemonCommand::Ping;
        let query_calls = Arc::new(AtomicUsize::new(0));
        let ensure_calls = Arc::new(AtomicUsize::new(0));
        let query_response = TldrDaemonResponse {
            status: "ok".to_string(),
            message: "pong".to_string(),
            analysis: None,
            semantic: None,
            snapshot: None,
            daemon_status: None,
            reindex_report: None,
        };

        let response = query_daemon_with_hooks(
            tempdir.path(),
            &command,
            {
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
            {
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
            {
                let query_calls = Arc::clone(&query_calls);
                move |_project_root, _command| {
                    let query_calls = Arc::clone(&query_calls);
                    Box::pin(async move {
                        query_calls.fetch_add(1, Ordering::SeqCst);
                        Ok(None)
                    })
                }
            },
            {
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

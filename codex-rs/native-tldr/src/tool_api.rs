use crate::TldrEngine;
use crate::api::AnalysisKind;
use crate::api::AnalysisRequest;
use crate::api::DiagnosticsRequest;
use crate::api::DoctorRequest;
use crate::api::ImportersRequest;
use crate::api::ImportsRequest;
use crate::api::SearchRequest;
use crate::daemon::TldrDaemonCommand;
use crate::daemon::TldrDaemonResponse;
use crate::daemon::daemon_health;
use crate::daemon::launch_lock_is_held;
use crate::lang_support::LanguageRegistry;
use crate::lang_support::SupportedLanguage;
use crate::lifecycle::DaemonLifecycleManager;
use crate::lifecycle::DaemonReadyResult;
use crate::lifecycle::QueryHooksResult;
use crate::load_tldr_config;
use crate::semantic::SemanticSearchRequest;
use crate::wire::daemon_response_payload;
use crate::wire::semantic_payload;
use anyhow::Result;
use once_cell::sync::Lazy;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::future::Future;
use std::path::Path;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::Duration;
use std::time::Instant;
use tokio::time::sleep;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TldrToolAction {
    Structure,
    Search,
    Extract,
    Imports,
    Importers,
    Context,
    Impact,
    Calls,
    Dead,
    Arch,
    ChangeImpact,
    Cfg,
    Dfg,
    Slice,
    Semantic,
    Diagnostics,
    Doctor,
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
    C,
    Cpp,
    Csharp,
    Elixir,
    Java,
    Typescript,
    Javascript,
    Lua,
    Luau,
    Python,
    Go,
    Php,
    Ruby,
    Scala,
    Swift,
    Zig,
}

impl From<TldrToolLanguage> for SupportedLanguage {
    fn from(value: TldrToolLanguage) -> Self {
        match value {
            TldrToolLanguage::Rust => SupportedLanguage::Rust,
            TldrToolLanguage::C => SupportedLanguage::C,
            TldrToolLanguage::Cpp => SupportedLanguage::Cpp,
            TldrToolLanguage::Csharp => SupportedLanguage::CSharp,
            TldrToolLanguage::Elixir => SupportedLanguage::Elixir,
            TldrToolLanguage::Java => SupportedLanguage::Java,
            TldrToolLanguage::Typescript => SupportedLanguage::TypeScript,
            TldrToolLanguage::Javascript => SupportedLanguage::JavaScript,
            TldrToolLanguage::Lua => SupportedLanguage::Lua,
            TldrToolLanguage::Luau => SupportedLanguage::Luau,
            TldrToolLanguage::Python => SupportedLanguage::Python,
            TldrToolLanguage::Go => SupportedLanguage::Go,
            TldrToolLanguage::Php => SupportedLanguage::Php,
            TldrToolLanguage::Ruby => SupportedLanguage::Ruby,
            TldrToolLanguage::Scala => SupportedLanguage::Scala,
            TldrToolLanguage::Swift => SupportedLanguage::Swift,
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
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paths: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub only_tools: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_lint: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_typecheck: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_issues: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub include_install_hints: Option<bool>,
}

impl Default for TldrToolCallParam {
    fn default() -> Self {
        Self {
            action: TldrToolAction::Ping,
            project: None,
            language: None,
            symbol: None,
            query: None,
            module: None,
            path: None,
            line: None,
            paths: None,
            only_tools: None,
            run_lint: None,
            run_typecheck: None,
            max_issues: None,
            include_install_hints: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TldrToolResult {
    pub text: String,
    pub structured_content: serde_json::Value,
}

pub type QueryDaemonFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<TldrDaemonResponse>>> + Send + 'a>>;
pub type EnsureDaemonFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;
pub type EnsureDaemonDetailedFuture<'a> =
    Pin<Box<dyn Future<Output = Result<DaemonReadyResult>> + Send + 'a>>;

static DAEMON_LIFECYCLE_MANAGER: Lazy<DaemonLifecycleManager> =
    Lazy::new(DaemonLifecycleManager::default);

pub async fn run_tldr_tool_with_hooks<Q, E>(
    args: TldrToolCallParam,
    query: Q,
    ensure_running: E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let project_root = resolve_project_root(args.project.as_deref())?;
    match args.action {
        TldrToolAction::Structure => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Structure,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Search => {
            let pattern = args
                .query
                .clone()
                .ok_or_else(|| anyhow::anyhow!("`query` is required for action=search"))?;
            let language = args.language.map(Into::into);
            run_search_tool(&project_root, language, pattern, &query, &ensure_running).await
        }
        TldrToolAction::Extract => {
            let path = args
                .path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("`path` is required for action=extract"))?;
            let language = required_or_inferred_language(&args, Some(path.as_str()))?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Extract,
                language,
                args.symbol,
                Some(path),
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Imports => {
            let path = args
                .path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("`path` is required for action=imports"))?;
            let language = required_or_inferred_language(&args, Some(path.as_str()))?;
            run_imports_tool(&project_root, language, path, &query, &ensure_running).await
        }
        TldrToolAction::Importers => {
            let module = args
                .module
                .clone()
                .ok_or_else(|| anyhow::anyhow!("`module` is required for action=importers"))?;
            let language = required_language(&args)?;
            run_importers_tool(&project_root, language, module, &query, &ensure_running).await
        }
        TldrToolAction::Context => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Context,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Impact => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Impact,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Calls => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Calls,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Dead => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Dead,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Arch => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Arch,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::ChangeImpact => {
            let paths = args
                .paths
                .clone()
                .filter(|value| !value.is_empty())
                .ok_or_else(|| anyhow::anyhow!("`paths` is required for action=change-impact"))?;
            let language = required_language(&args)?;
            run_change_impact_tool(&project_root, language, paths, &query, &ensure_running).await
        }
        TldrToolAction::Cfg => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Cfg,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Dfg => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Dfg,
                language,
                args.symbol,
                None,
                None,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Slice => {
            let path = args
                .path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("`path` is required for action=slice"))?;
            let line = args
                .line
                .ok_or_else(|| anyhow::anyhow!("`line` is required for action=slice"))?;
            let language = required_or_inferred_language(&args, Some(path.as_str()))?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Slice,
                language,
                args.symbol,
                Some(path),
                Some(line),
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Semantic => {
            run_semantic_tool(&project_root, args, &query, &ensure_running).await
        }
        TldrToolAction::Diagnostics => {
            let path = args
                .path
                .clone()
                .ok_or_else(|| anyhow::anyhow!("`path` is required for action=diagnostics"))?;
            let language = required_or_inferred_language(&args, Some(path.as_str()))?;
            run_diagnostics_tool(
                &project_root,
                language,
                path,
                &args,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Doctor => run_doctor_tool(&project_root, &args),
        TldrToolAction::Ping => {
            run_daemon_tool(
                &project_root,
                TldrDaemonCommand::Ping,
                "ping",
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Warm => {
            run_daemon_tool(
                &project_root,
                TldrDaemonCommand::Warm,
                "warm",
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Snapshot => {
            run_daemon_tool(
                &project_root,
                TldrDaemonCommand::Snapshot,
                "snapshot",
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Status => {
            run_daemon_tool(
                &project_root,
                TldrDaemonCommand::Status,
                "status",
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Notify => {
            let path = args
                .path
                .map(PathBuf::from)
                .ok_or_else(|| anyhow::anyhow!("`path` is required for action=notify"))?;
            run_daemon_tool(
                &project_root,
                TldrDaemonCommand::Notify { path },
                "notify",
                &query,
                &ensure_running,
            )
            .await
        }
    }
}

async fn run_analysis_tool<Q, E>(
    project_root: &Path,
    action: TldrToolAction,
    language: SupportedLanguage,
    symbol: Option<String>,
    path: Option<String>,
    line: Option<usize>,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let kind = match action {
        TldrToolAction::Structure => AnalysisKind::Ast,
        TldrToolAction::Extract => AnalysisKind::Extract,
        TldrToolAction::Context => AnalysisKind::CallGraph,
        TldrToolAction::Impact => AnalysisKind::Impact,
        TldrToolAction::Calls => AnalysisKind::Calls,
        TldrToolAction::Dead => AnalysisKind::Dead,
        TldrToolAction::Arch => AnalysisKind::Arch,
        TldrToolAction::Cfg => AnalysisKind::Cfg,
        TldrToolAction::Dfg => AnalysisKind::Dfg,
        TldrToolAction::Slice => AnalysisKind::Slice,
        _ => unreachable!("analysis action must map to AnalysisKind"),
    };
    let request = AnalysisRequest {
        kind,
        language,
        symbol: symbol.clone(),
        path: path.clone(),
        line,
        paths: Vec::new(),
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(kind, language, symbol.as_deref(), path.as_deref(), line),
            request: request.clone(),
        },
        query,
        ensure_running,
    )
    .await?;
    let support = LanguageRegistry::support_for(language);
    let (source, message, analysis) = if let Some(response) = daemon_response {
        let analysis = response
            .analysis
            .ok_or_else(|| anyhow::anyhow!("daemon response missing analysis payload"))?;
        ("daemon", response.message, analysis)
    } else {
        let config = load_tldr_config(project_root)?;
        let engine = TldrEngine::builder(project_root.to_path_buf())
            .with_config(config)
            .build();
        let response = engine.analyze(request)?;
        (
            "local",
            "daemon unavailable; used local engine".to_string(),
            response,
        )
    };

    let structured_content = json!({
        "action": action_name(&action),
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "supportLevel": format!("{:?}", support.support_level),
        "symbolExtractor": support.symbol_extractor.as_str(),
        "relationshipSupport": support.symbol_relationship_support.as_str(),
        "fallbackStrategy": support.fallback_strategy,
        "summary": analysis.summary,
        "symbol": symbol,
        "path": path,
        "line": line,
        "paths": serde_json::Value::Null,
        "analysis": analysis,
    });
    let text = format!(
        "{} {} via {source}: {}",
        action_name(&action),
        language.as_str(),
        structured_content["summary"].as_str().unwrap_or_default()
    );
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

async fn run_semantic_tool<Q, E>(
    project_root: &Path,
    args: TldrToolCallParam,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let language = required_language(&args)?;
    let search_query = args
        .query
        .ok_or_else(|| anyhow::anyhow!("`query` is required for action=semantic"))?;
    let request = SemanticSearchRequest {
        language,
        query: search_query.clone(),
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Semantic {
            request: request.clone(),
        },
        query,
        ensure_running,
    )
    .await?;
    let (response, source) = if let Some(response) = daemon_response {
        if let Some(semantic) = response.semantic {
            (semantic, "daemon")
        } else {
            (
                run_local_semantic(project_root, language, &search_query)?,
                "local",
            )
        }
    } else {
        (
            run_local_semantic(project_root, language, &search_query)?,
            "local",
        )
    };

    let mut structured_content =
        semantic_payload(Some("semantic"), project_root, language, source, &response);
    let semantic = semantic_result_payload(&structured_content);
    if let Some(object) = structured_content.as_object_mut() {
        object.insert("semantic".to_string(), semantic);
    }

    Ok(TldrToolResult {
        text: format!(
            "semantic {} enabled={} via {source}: {}",
            language.as_str(),
            response.enabled,
            response.message
        ),
        structured_content,
    })
}

fn run_local_semantic(
    project_root: &Path,
    language: SupportedLanguage,
    query: &str,
) -> Result<crate::semantic::SemanticSearchResponse> {
    let config = load_tldr_config(project_root)?;
    let engine = TldrEngine::builder(project_root.to_path_buf())
        .with_config(config)
        .build();
    engine.semantic_search(SemanticSearchRequest {
        language,
        query: query.to_string(),
    })
}

async fn run_daemon_tool<Q, E>(
    project_root: &Path,
    command: TldrDaemonCommand,
    action: &str,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let Some(response) =
        query_daemon_with_hooks(project_root, &command, query, ensure_running).await?
    else {
        return Err(daemon_unavailable_error(project_root));
    };
    let structured_content = daemon_response_payload(action, project_root, &response);
    let message = structured_content
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ok");
    let text = format!("{action}: {message}");
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

fn daemon_unavailable_error(project_root: &Path) -> anyhow::Error {
    daemon_unavailable_error_for_project(project_root, None)
}

pub fn daemon_unavailable_error_for_project(
    project_root: &Path,
    ready_result: Option<&DaemonReadyResult>,
) -> anyhow::Error {
    let project = project_root.display();
    if let Some(failure) = ready_result.and_then(|value| value.structured_failure.as_ref()) {
        if let Some(hint) = &failure.retry_hint {
            return anyhow::anyhow!(
                "native-tldr daemon is unavailable for {project}: {} (hint: {hint})",
                failure.reason
            );
        }
        return anyhow::anyhow!(
            "native-tldr daemon is unavailable for {project}: {}",
            failure.reason
        );
    }
    match daemon_health(project_root) {
        Ok(health) => {
            if let Some(failure) = health.structured_failure {
                if let Some(hint) = failure.retry_hint {
                    anyhow::anyhow!(
                        "native-tldr daemon is unavailable for {project}: {} (hint: {hint})",
                        failure.reason
                    )
                } else {
                    anyhow::anyhow!(
                        "native-tldr daemon is unavailable for {project}: {}",
                        failure.reason
                    )
                }
            } else {
                anyhow::anyhow!("native-tldr daemon is unavailable for {project}")
            }
        }
        Err(_) => anyhow::anyhow!("native-tldr daemon is unavailable for {project}"),
    }
}

async fn run_search_tool<Q, E>(
    project_root: &Path,
    language: Option<SupportedLanguage>,
    pattern: String,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let request = SearchRequest {
        pattern: pattern.clone(),
        language,
        max_results: 100,
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Search {
            request: request.clone(),
        },
        query,
        ensure_running,
    )
    .await?;
    let (source, message, response) = if let Some(response) = daemon_response {
        let search = response
            .search
            .ok_or_else(|| anyhow::anyhow!("daemon response missing search payload"))?;
        ("daemon", response.message, search)
    } else {
        let config = load_tldr_config(project_root)?;
        let engine = TldrEngine::builder(project_root.to_path_buf())
            .with_config(config)
            .build();
        let response = engine.search(request)?;
        (
            "local",
            "daemon unavailable; used local engine".to_string(),
            response,
        )
    };
    let structured_content = json!({
        "action": "search",
        "project": project_root,
        "language": language.map(SupportedLanguage::as_str),
        "source": source,
        "message": message,
        "pattern": pattern,
        "search": response,
    });
    let text = format!("search via {source}: {} matches", response.matches.len());
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

pub async fn query_daemon_with_hooks<Q, E>(
    project_root: &Path,
    command: &TldrDaemonCommand,
    query: &Q,
    ensure_running: &E,
) -> Result<Option<TldrDaemonResponse>>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    DAEMON_LIFECYCLE_MANAGER
        .query_or_spawn_with_hooks(project_root, command, query, ensure_running)
        .await
}

pub async fn query_daemon_with_hooks_detailed<Q, E>(
    project_root: &Path,
    command: &TldrDaemonCommand,
    query: &Q,
    ensure_running: E,
) -> Result<QueryHooksResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonDetailedFuture<'a>,
{
    DAEMON_LIFECYCLE_MANAGER
        .query_or_spawn_with_hooks_detailed(project_root, command, query, ensure_running)
        .await
}

pub async fn wait_for_external_daemon(project_root: &Path) -> Result<bool> {
    if !crate::daemon::daemon_lock_is_held(project_root)? && !launch_lock_is_held(project_root)? {
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

pub fn daemon_metadata_looks_alive(project_root: &Path) -> bool {
    daemon_health(project_root)
        .map(|health| health.healthy)
        .unwrap_or(false)
}

pub fn structured_failure_error_type(kind: &crate::daemon::StructuredFailureKind) -> &'static str {
    match kind {
        crate::daemon::StructuredFailureKind::DaemonUnavailable => "daemon_unavailable",
        crate::daemon::StructuredFailureKind::DaemonStarting => "daemon_starting",
        crate::daemon::StructuredFailureKind::StaleSocket => "stale_socket",
        crate::daemon::StructuredFailureKind::StalePid => "stale_pid",
        crate::daemon::StructuredFailureKind::DaemonUnhealthy => "daemon_unhealthy",
    }
}

pub fn degraded_mode_name(kind: &crate::daemon::DegradedModeKind) -> &'static str {
    match kind {
        crate::daemon::DegradedModeKind::DiagnosticOnly => "diagnostic_only",
    }
}

pub fn action_name(action: &TldrToolAction) -> &'static str {
    match action {
        TldrToolAction::Structure => "structure",
        TldrToolAction::Search => "search",
        TldrToolAction::Extract => "extract",
        TldrToolAction::Imports => "imports",
        TldrToolAction::Importers => "importers",
        TldrToolAction::Context => "context",
        TldrToolAction::Impact => "impact",
        TldrToolAction::Calls => "calls",
        TldrToolAction::Dead => "dead",
        TldrToolAction::Arch => "arch",
        TldrToolAction::ChangeImpact => "change-impact",
        TldrToolAction::Cfg => "cfg",
        TldrToolAction::Dfg => "dfg",
        TldrToolAction::Slice => "slice",
        TldrToolAction::Semantic => "semantic",
        TldrToolAction::Diagnostics => "diagnostics",
        TldrToolAction::Doctor => "doctor",
        TldrToolAction::Ping => "ping",
        TldrToolAction::Warm => "warm",
        TldrToolAction::Snapshot => "snapshot",
        TldrToolAction::Status => "status",
        TldrToolAction::Notify => "notify",
    }
}

pub fn analysis_cache_key(
    kind: AnalysisKind,
    language: SupportedLanguage,
    symbol: Option<&str>,
    path: Option<&str>,
    line: Option<usize>,
) -> String {
    let symbol = symbol.unwrap_or("*");
    let path = path.unwrap_or("*");
    let line = line.map_or("*".to_string(), |value| value.to_string());
    format!("{}:{kind:?}:{symbol}:{path}:{line}", language.as_str())
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

fn required_or_inferred_language(
    args: &TldrToolCallParam,
    path: Option<&str>,
) -> Result<SupportedLanguage> {
    if let Some(language) = args.language {
        return Ok(language.into());
    }
    let path = path.ok_or_else(|| {
        anyhow::anyhow!(
            "`language` is required for action={}",
            action_name(&args.action)
        )
    })?;
    SupportedLanguage::from_path(Path::new(path)).ok_or_else(|| {
        anyhow::anyhow!(
            "`language` is required for action={} when path extension is unsupported",
            action_name(&args.action)
        )
    })
}

fn resolve_project_root(project: Option<&str>) -> Result<PathBuf> {
    let project_root = match project {
        Some(project) => PathBuf::from(project),
        None => std::env::current_dir()?,
    };
    Ok(project_root.canonicalize()?)
}

fn semantic_result_payload(payload: &serde_json::Value) -> serde_json::Value {
    let mut semantic = payload.clone();
    if let Some(object) = semantic.as_object_mut() {
        for key in ["action", "project", "language", "source"] {
            object.remove(key);
        }
    }
    semantic
}

async fn run_imports_tool<Q, E>(
    project_root: &Path,
    language: SupportedLanguage,
    path: String,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let request = ImportsRequest {
        language,
        path: path.clone(),
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Imports {
            request: request.clone(),
        },
        query,
        ensure_running,
    )
    .await?;
    let (source, message, response) = if let Some(response) = daemon_response {
        let imports = response
            .imports
            .ok_or_else(|| anyhow::anyhow!("daemon response missing imports payload"))?;
        ("daemon", response.message, imports)
    } else {
        let config = load_tldr_config(project_root)?;
        let engine = TldrEngine::builder(project_root.to_path_buf())
            .with_config(config)
            .build();
        let response = engine.imports(request)?;
        (
            "local",
            "daemon unavailable; used local engine".to_string(),
            response,
        )
    };
    let structured_content = json!({
        "action": "imports",
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "path": path,
        "imports": {
            "path": response.path,
            "language": response.language.as_str(),
            "indexedFiles": response.indexed_files,
            "imports": response.imports,
        },
    });
    let import_count = structured_content["imports"]["imports"]
        .as_array()
        .map_or(0, Vec::len);
    let text = format!(
        "imports {} via {source}: {import_count} imports",
        language.as_str()
    );
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

async fn run_diagnostics_tool<Q, E>(
    project_root: &Path,
    language: SupportedLanguage,
    path: String,
    args: &TldrToolCallParam,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let request = DiagnosticsRequest {
        language,
        path: path.clone(),
        only_tools: args.only_tools.clone().unwrap_or_default(),
        run_lint: args.run_lint.unwrap_or(true),
        run_typecheck: args.run_typecheck.unwrap_or(true),
        max_issues: args.max_issues.unwrap_or(50).max(1),
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Diagnostics {
            request: request.clone(),
        },
        query,
        ensure_running,
    )
    .await?;
    let (source, message, response) = if let Some(response) = daemon_response {
        let diagnostics = response
            .diagnostics
            .ok_or_else(|| anyhow::anyhow!("daemon response missing diagnostics payload"))?;
        ("daemon", response.message, diagnostics)
    } else {
        let config = load_tldr_config(project_root)?;
        let engine = TldrEngine::builder(project_root.to_path_buf())
            .with_config(config)
            .build();
        let response = engine.diagnostics(request)?;
        (
            "local",
            "daemon unavailable; used local engine".to_string(),
            response,
        )
    };
    let structured_content = json!({
        "action": "diagnostics",
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "path": path,
        "diagnostics": response,
    });
    let text = format!(
        "diagnostics {} via {source}: {} issues",
        language.as_str(),
        response.diagnostics.len()
    );
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

fn run_doctor_tool(project_root: &Path, args: &TldrToolCallParam) -> Result<TldrToolResult> {
    let config = load_tldr_config(project_root)?;
    let engine = TldrEngine::builder(project_root.to_path_buf())
        .with_config(config)
        .build();
    let response = engine.doctor(DoctorRequest {
        language: args.language.map(Into::into),
        only_tools: args.only_tools.clone().unwrap_or_default(),
        include_install_hints: args.include_install_hints.unwrap_or(true),
    });
    let structured_content = json!({
        "action": "doctor",
        "project": project_root,
        "language": args.language.map(SupportedLanguage::from).map(SupportedLanguage::as_str),
        "message": response.message,
        "tools": response.tools,
        "doctor": response,
    });
    let text = structured_content["message"]
        .as_str()
        .map_or_else(|| "doctor".to_string(), str::to_string);
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

async fn run_importers_tool<Q, E>(
    project_root: &Path,
    language: SupportedLanguage,
    module: String,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let request = ImportersRequest {
        language,
        module: module.clone(),
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Importers {
            request: request.clone(),
        },
        query,
        ensure_running,
    )
    .await?;
    let (source, message, response) = if let Some(response) = daemon_response {
        let importers = response
            .importers
            .ok_or_else(|| anyhow::anyhow!("daemon response missing importers payload"))?;
        ("daemon", response.message, importers)
    } else {
        let config = load_tldr_config(project_root)?;
        let engine = TldrEngine::builder(project_root.to_path_buf())
            .with_config(config)
            .build();
        let response = engine.importers(request)?;
        (
            "local",
            "daemon unavailable; used local engine".to_string(),
            response,
        )
    };
    let structured_content = json!({
        "action": "importers",
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "module": module,
        "importers": {
            "module": response.module,
            "language": response.language.as_str(),
            "indexedFiles": response.indexed_files,
            "matches": response.matches,
        },
    });
    let match_count = structured_content["importers"]["matches"]
        .as_array()
        .map_or(0, Vec::len);
    let text = format!(
        "importers {} via {source}: {match_count} matches",
        language.as_str()
    );
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

async fn run_change_impact_tool<Q, E>(
    project_root: &Path,
    language: SupportedLanguage,
    paths: Vec<String>,
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let request = AnalysisRequest {
        kind: AnalysisKind::ChangeImpact,
        language,
        symbol: None,
        path: None,
        line: None,
        paths: paths.clone(),
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(AnalysisKind::ChangeImpact, language, None, None, None),
            request: request.clone(),
        },
        query,
        ensure_running,
    )
    .await?;
    let support = LanguageRegistry::support_for(language);
    let (source, message, analysis) = if let Some(response) = daemon_response {
        let analysis = response
            .analysis
            .ok_or_else(|| anyhow::anyhow!("daemon response missing analysis payload"))?;
        ("daemon", response.message, analysis)
    } else {
        let config = load_tldr_config(project_root)?;
        let engine = TldrEngine::builder(project_root.to_path_buf())
            .with_config(config)
            .build();
        let response = engine.analyze(request)?;
        (
            "local",
            "daemon unavailable; used local engine".to_string(),
            response,
        )
    };

    let structured_content = json!({
        "action": "change-impact",
        "project": project_root,
        "language": language.as_str(),
        "source": source,
        "message": message,
        "supportLevel": format!("{:?}", support.support_level),
        "symbolExtractor": support.symbol_extractor.as_str(),
        "relationshipSupport": support.symbol_relationship_support.as_str(),
        "fallbackStrategy": support.fallback_strategy,
        "summary": analysis.summary,
        "paths": paths,
        "analysis": analysis,
    });
    let text = format!(
        "change-impact {} via {source}: {}",
        language.as_str(),
        structured_content["summary"].as_str().unwrap_or_default()
    );
    Ok(TldrToolResult {
        text,
        structured_content,
    })
}

pub fn tldr_tool_output_schema() -> serde_json::Value {
    serde_json::from_str(
        r##"{
          "type": "object",
          "$defs": {
            "analysisUnit": {
              "type": "object",
              "properties": {
                "path": { "type": "string" },
                "line": { "type": "integer" },
                "span_end_line": { "type": "integer" },
                "symbol": { "type": ["string", "null"] },
                "qualified_symbol": { "type": ["string", "null"] },
                "kind": { "type": "string" },
                "owner_symbol": { "type": ["string", "null"] },
                "owner_kind": { "type": ["string", "null"] },
                "implemented_trait": { "type": ["string", "null"] },
                "module_path": { "type": "array", "items": { "type": "string" } },
                "visibility": { "type": ["string", "null"] },
                "signature": { "type": ["string", "null"] },
                "calls": { "type": "array", "items": { "type": "string" } },
                "called_by": { "type": "array", "items": { "type": "string" } },
                "references": { "type": "array", "items": { "type": "string" } },
                "imports": { "type": "array", "items": { "type": "string" } },
                "dependencies": { "type": "array", "items": { "type": "string" } },
                "cfg_summary": { "type": "string" },
                "dfg_summary": { "type": "string" }
              }
            },
            "analysisDetails": {
              "type": "object",
              "properties": {
                "indexed_files": { "type": "integer" },
                "total_symbols": { "type": "integer" },
                "symbol_query": { "type": ["string", "null"] },
                "truncated": { "type": "boolean" },
                "change_paths": {
                  "type": "array",
                  "items": { "type": "string" }
                },
                "slice_target": {
                  "type": ["object", "null"],
                  "properties": {
                    "path": { "type": "string" },
                    "symbol": { "type": ["string", "null"] },
                    "line": { "type": "integer" },
                    "direction": { "type": "string" }
                  }
                },
                "slice_lines": {
                  "type": "array",
                  "items": { "type": "integer" }
                },
                "overview": { "$ref": "#/$defs/analysisOverview" },
                "files": {
                  "type": "array",
                  "items": { "$ref": "#/$defs/analysisFile" }
                },
                "nodes": {
                  "type": "array",
                  "items": { "$ref": "#/$defs/analysisNode" }
                },
                "edges": {
                  "type": "array",
                  "items": { "$ref": "#/$defs/analysisEdge" }
                },
                "symbol_index": {
                  "type": "array",
                  "items": { "$ref": "#/$defs/analysisSymbolIndexEntry" }
                },
                "units": {
                  "type": "array",
                  "items": { "$ref": "#/$defs/analysisUnit" }
                }
              }
            },
            "analysisOverview": {
              "type": "object",
              "properties": {
                "kinds": {
                  "type": "array",
                  "items": { "$ref": "#/$defs/analysisCount" }
                },
                "outgoing_edges": { "type": "integer" },
                "incoming_edges": { "type": "integer" },
                "reference_count": { "type": "integer" },
                "import_count": { "type": "integer" }
              }
            },
            "analysisCount": {
              "type": "object",
              "properties": {
                "name": { "type": "string" },
                "count": { "type": "integer" }
              }
            },
            "analysisFile": {
              "type": "object",
              "properties": {
                "path": { "type": "string" },
                "symbol_count": { "type": "integer" },
                "kinds": {
                  "type": "array",
                  "items": { "$ref": "#/$defs/analysisCount" }
                }
              }
            },
            "analysisEdge": {
              "type": "object",
              "properties": {
                "from": { "type": "string" },
                "to": { "type": "string" },
                "kind": { "type": "string" }
              }
            },
            "analysisNode": {
              "type": "object",
              "properties": {
                "id": { "type": "string" },
                "label": { "type": "string" },
                "kind": { "type": "string" },
                "path": { "type": ["string", "null"] },
                "line": { "type": ["integer", "null"] },
                "signature": { "type": ["string", "null"] }
              }
            },
            "analysisSymbolIndexEntry": {
              "type": "object",
              "properties": {
                "symbol": { "type": "string" },
                "node_ids": {
                  "type": "array",
                  "items": { "type": "string" }
                }
              }
            },
            "importMatch": {
              "type": "object",
              "properties": {
                "path": { "type": "string" },
                "line": { "type": "integer" },
                "symbol": { "type": ["string", "null"] },
                "import": { "type": "string" }
              }
            },
            "analysis": {
              "type": "object",
              "properties": {
                "kind": { "type": "string" },
                "summary": { "type": "string" },
                "details": { "$ref": "#/$defs/analysisDetails" }
              },
              "required": ["kind", "summary", "details"]
            },
            "reindexReport": {
              "type": ["object", "null"],
              "properties": {
                "status": { "type": "string" },
                "languages": { "type": "array", "items": { "type": "string" } },
                "indexed_files": { "type": "integer" },
                "indexed_units": { "type": "integer" },
                "truncated": { "type": "boolean" },
                "started_at": { "type": "string" },
                "finished_at": { "type": "string" },
                "message": { "type": "string" },
                "embedding_enabled": { "type": "boolean" },
                "embedding_dimensions": { "type": "integer" }
              }
            },
            "structuredFailure": {
              "type": ["object", "null"],
              "properties": {
                "error_type": { "type": "string" },
                "reason": { "type": "string" },
                "retryable": { "type": "boolean" },
                "retry_hint": { "type": ["string", "null"] }
              }
            },
            "degradedMode": {
              "type": ["object", "null"],
              "properties": {
                "is_degraded": { "type": "boolean" },
                "mode": { "type": "string" },
                "fallback_path": { "type": "string" },
                "reason": { "type": ["string", "null"] }
              }
            },
            "semantic": {
              "type": "object",
              "properties": {
                "query": { "type": "string" },
                "enabled": { "type": "boolean" },
                "indexedFiles": { "type": "integer" },
                "truncated": { "type": "boolean" },
                "embeddingUsed": { "type": "boolean" },
                "message": { "type": "string" },
                "degradedMode": { "$ref": "#/$defs/degradedMode" },
                "matches": {
                  "type": "array",
                    "items": {
                    "type": "object",
                    "properties": {
                      "symbol": { "type": ["string", "null"] },
                      "qualifiedSymbol": { "type": ["string", "null"] },
                      "kind": { "type": "string" },
                      "signature": { "type": ["string", "null"] },
                      "path": { "type": "string" },
                      "line": { "type": "integer" },
                      "snippet": { "type": "string" },
                      "embedding_score": { "type": ["number", "null"] }
                    }
                  }
                }
              }
            },
            "analysisResult": {
              "type": "object",
              "properties": {
                "action": {
                  "enum": ["structure", "extract", "context", "impact", "calls", "dead", "arch", "change-impact", "cfg", "dfg", "slice"]
                },
                "project": { "type": "string" },
                "language": { "type": "string" },
                "source": { "type": "string" },
                "message": { "type": "string" },
                "supportLevel": { "type": "string" },
                "symbolExtractor": { "type": "string" },
                "relationshipSupport": { "type": "string" },
                "fallbackStrategy": { "type": "string" },
                "summary": { "type": "string" },
                "symbol": { "type": ["string", "null"] },
                "path": { "type": ["string", "null"] },
                "line": { "type": ["integer", "null"] },
                "paths": {
                  "type": ["array", "null"],
                  "items": { "type": "string" }
                },
                "analysis": { "$ref": "#/$defs/analysis" }
              },
              "required": [
                "action",
                "project",
                "language",
                "source",
                "message",
                "supportLevel",
                "symbolExtractor",
                "relationshipSupport",
                "fallbackStrategy",
                "summary",
                "analysis"
              ]
            },
            "semanticResult": {
              "type": "object",
              "properties": {
                "action": { "const": "semantic" },
                "project": { "type": "string" },
                "language": { "type": "string" },
                "source": { "type": "string" },
                "query": { "type": "string" },
                "enabled": { "type": "boolean" },
                "indexedFiles": { "type": "integer" },
                "truncated": { "type": "boolean" },
                "embeddingUsed": { "type": "boolean" },
                "message": { "type": "string" },
                "matches": {
                  "type": "array",
                  "items": {
                    "type": "object",
                    "properties": {
                      "symbol": { "type": ["string", "null"] },
                      "qualifiedSymbol": { "type": ["string", "null"] },
                      "kind": { "type": "string" },
                      "signature": { "type": ["string", "null"] },
                      "path": { "type": "string" },
                      "line": { "type": "integer" },
                      "snippet": { "type": "string" },
                      "embedding_score": { "type": ["number", "null"] }
                    }
                  }
                },
                "semantic": { "$ref": "#/$defs/semantic" }
              },
              "required": [
                "action",
                "project",
                "language",
                "source",
                "query",
                "enabled",
                "indexedFiles",
                "truncated",
                "embeddingUsed",
                "message",
                "degradedMode",
                "matches",
                "semantic"
              ]
            },
            "importsResult": {
              "type": "object",
              "properties": {
                "action": { "const": "imports" },
                "project": { "type": "string" },
                "language": { "type": "string" },
                "source": { "type": "string" },
                "message": { "type": "string" },
                "path": { "type": "string" },
                "imports": {
                  "type": "object",
                  "properties": {
                    "path": { "type": "string" },
                    "language": { "type": "string" },
                    "indexedFiles": { "type": "integer" },
                    "imports": {
                      "type": "array",
                      "items": { "type": "string" }
                    }
                  }
                }
              },
              "required": [
                "action",
                "project",
                "language",
                "source",
                "message",
                "path",
                "imports"
              ]
            },
            "importersResult": {
              "type": "object",
              "properties": {
                "action": { "const": "importers" },
                "project": { "type": "string" },
                "language": { "type": "string" },
                "source": { "type": "string" },
                "message": { "type": "string" },
                "module": { "type": "string" },
                "importers": {
                  "type": "object",
                  "properties": {
                    "module": { "type": "string" },
                    "language": { "type": "string" },
                    "indexedFiles": { "type": "integer" },
                    "matches": {
                      "type": "array",
                      "items": { "$ref": "#/$defs/importMatch" }
                    }
                  }
                }
              },
              "required": [
                "action",
                "project",
                "language",
                "source",
                "message",
                "module",
                "importers"
              ]
            },
            "searchResult": {
              "type": "object",
              "properties": {
                "action": { "const": "search" },
                "project": { "type": "string" },
                "language": { "type": ["string", "null"] },
                "source": { "type": "string" },
                "message": { "type": "string" },
                "pattern": { "type": "string" },
                "search": {
                  "type": "object",
                  "properties": {
                    "pattern": { "type": "string" },
                    "indexed_files": { "type": "integer" },
                    "truncated": { "type": "boolean" },
                    "matches": {
                      "type": "array",
                      "items": {
                        "type": "object",
                        "properties": {
                          "path": { "type": "string" },
                          "line": { "type": "integer" },
                          "content": { "type": "string" }
                        }
                      }
                    }
                  }
                }
              },
              "required": [
                "action",
                "project",
                "language",
                "source",
                "message",
                "pattern",
                "search"
              ]
            },
            "diagnosticsResult": {
              "type": "object",
              "properties": {
                "action": { "const": "diagnostics" },
                "project": { "type": "string" },
                "language": { "type": "string" },
                "source": { "type": "string" },
                "message": { "type": "string" },
                "path": { "type": "string" },
                "diagnostics": {
                  "type": "object",
                  "properties": {
                    "language": { "type": "string" },
                    "path": { "type": "string" },
                    "tools": {
                      "type": "array",
                      "items": {
                        "type": "object",
                        "properties": {
                          "tool": { "type": "string" },
                          "available": { "type": "boolean" },
                          "kind": { "type": "string" }
                        }
                      }
                    },
                    "diagnostics": {
                      "type": "array",
                      "items": {
                        "type": "object",
                        "properties": {
                          "path": { "type": "string" },
                          "line": { "type": "integer" },
                          "column": { "type": "integer" },
                          "severity": { "type": "string" },
                          "message": { "type": "string" },
                          "code": { "type": ["string", "null"] },
                          "source": { "type": "string" }
                        }
                      }
                    },
                    "truncated": { "type": "boolean" },
                    "message": { "type": "string" }
                  }
                }
              },
              "required": [
                "action",
                "project",
                "language",
                "source",
                "message",
                "path",
                "diagnostics"
              ]
            },
            "doctorResult": {
              "type": "object",
              "properties": {
                "action": { "const": "doctor" },
                "project": { "type": "string" },
                "language": { "type": ["string", "null"] },
                "message": { "type": "string" },
                "tools": {
                  "type": "array",
                  "items": {
                    "type": "object",
                    "properties": {
                      "tool": { "type": "string" },
                      "available": { "type": "boolean" },
                      "purpose": { "type": "string" },
                      "languages": {
                        "type": "array",
                        "items": { "type": "string" }
                      },
                      "install_hint": { "type": ["string", "null"] }
                    }
                  }
                },
                "doctor": {
                  "type": "object",
                  "properties": {
                    "message": { "type": "string" },
                    "tools": {
                      "type": "array",
                      "items": {
                        "type": "object",
                        "properties": {
                          "tool": { "type": "string" },
                          "available": { "type": "boolean" },
                          "purpose": { "type": "string" },
                          "languages": {
                            "type": "array",
                            "items": { "type": "string" }
                          },
                          "install_hint": { "type": ["string", "null"] }
                        }
                      }
                    }
                  }
                }
              },
              "required": [
                "action",
                "project",
                "message",
                "tools",
                "doctor"
              ]
            },
            "daemonResult": {
              "type": "object",
              "properties": {
                "action": {
                  "enum": ["ping", "warm", "snapshot", "status", "notify"]
                },
                "project": { "type": "string" },
                "status": { "type": "string" },
                "message": { "type": "string" },
                "structuredFailure": { "$ref": "#/$defs/structuredFailure" },
                "degradedMode": { "$ref": "#/$defs/degradedMode" },
                "snapshot": {
                  "type": "object",
                  "properties": {
                    "cached_entries": { "type": "integer" },
                    "dirty_files": { "type": "integer" },
                    "dirty_file_threshold": { "type": "integer" },
                    "reindex_pending": { "type": "boolean" },
                    "background_reindex_in_progress": { "type": "boolean" },
                    "last_query_at": { "type": ["string", "null"] },
                    "last_reindex": { "$ref": "#/$defs/reindexReport" },
                    "last_reindex_attempt": { "$ref": "#/$defs/reindexReport" },
                    "lastStructuredFailure": { "$ref": "#/$defs/structuredFailure" },
                    "degradedModeActive": { "type": "boolean" },
                    "last_warm": {
                      "type": "object",
                      "properties": {
                        "status": { "type": "string" },
                        "languages": {
                          "type": "array",
                          "items": { "type": "string" }
                        },
                        "started_at": { "type": "string" },
                        "finished_at": { "type": "string" },
                        "message": { "type": "string" }
                      }
                    }
                  }
                },
                "daemonStatus": {
                  "type": "object",
                  "properties": {
                    "project_root": { "type": "string" },
                    "socket_path": { "type": "string" },
                    "pid_path": { "type": "string" },
                    "lock_path": { "type": "string" },
                    "socket_exists": { "type": "boolean" },
                    "pid_is_live": { "type": "boolean" },
                    "lock_is_held": { "type": "boolean" },
                    "healthy": { "type": "boolean" },
                    "stale_socket": { "type": "boolean" },
                    "stale_pid": { "type": "boolean" },
                    "health_reason": { "type": ["string", "null"] },
                    "recovery_hint": { "type": ["string", "null"] },
                    "structuredFailure": { "$ref": "#/$defs/structuredFailure" },
                    "degradedMode": { "$ref": "#/$defs/degradedMode" },
                    "semantic_reindex_pending": { "type": "boolean" },
                    "semantic_reindex_in_progress": { "type": "boolean" },
                    "last_query_at": { "type": ["string", "null"] },
                    "config": {
                      "type": "object",
                      "properties": {
                        "auto_start": { "type": "boolean" },
                        "socket_mode": { "type": "string" },
                        "semantic_enabled": { "type": "boolean" },
                        "semantic_auto_reindex_threshold": { "type": "integer" },
                        "session_dirty_file_threshold": { "type": "integer" },
                        "session_idle_timeout_secs": { "type": "integer" }
                      }
                    }
                  }
                },
                "reindexReport": { "$ref": "#/$defs/reindexReport" }
              },
              "required": ["action", "project", "status", "message", "structuredFailure", "degradedMode"]
            }
          },
          "oneOf": [
            { "$ref": "#/$defs/analysisResult" },
            { "$ref": "#/$defs/searchResult" },
            { "$ref": "#/$defs/importsResult" },
            { "$ref": "#/$defs/importersResult" },
            { "$ref": "#/$defs/semanticResult" },
            { "$ref": "#/$defs/diagnosticsResult" },
            { "$ref": "#/$defs/doctorResult" },
            { "$ref": "#/$defs/daemonResult" }
          ]
        }"##,
    )
    .expect("output schema literal should parse")
}

#[cfg(test)]
mod tests {
    use super::TldrToolAction;
    use super::TldrToolCallParam;
    use super::TldrToolLanguage;
    use super::action_name;
    use super::query_daemon_with_hooks_detailed;
    use super::run_tldr_tool_with_hooks;
    use crate::daemon::DegradedMode;
    use crate::daemon::DegradedModeKind;
    use crate::daemon::StructuredFailure;
    use crate::daemon::StructuredFailureKind;
    use crate::daemon::TldrDaemonConfigSummary;
    use crate::daemon::TldrDaemonResponse;
    use crate::daemon::TldrDaemonStatus;
    use crate::lang_support::SupportedLanguage;
    use crate::lifecycle::DaemonReadyResult;
    use crate::semantic::SemanticReindexReport;
    use crate::semantic::SemanticReindexStatus;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use std::time::SystemTime;
    use tempfile::tempdir;

    #[test]
    fn action_name_covers_analysis_actions() {
        assert_eq!(action_name(&TldrToolAction::Structure), "structure");
        assert_eq!(action_name(&TldrToolAction::Search), "search");
        assert_eq!(action_name(&TldrToolAction::Extract), "extract");
        assert_eq!(action_name(&TldrToolAction::Imports), "imports");
        assert_eq!(action_name(&TldrToolAction::Importers), "importers");
        assert_eq!(action_name(&TldrToolAction::Context), "context");
        assert_eq!(action_name(&TldrToolAction::Impact), "impact");
        assert_eq!(action_name(&TldrToolAction::Calls), "calls");
        assert_eq!(action_name(&TldrToolAction::Dead), "dead");
        assert_eq!(action_name(&TldrToolAction::Arch), "arch");
        assert_eq!(action_name(&TldrToolAction::ChangeImpact), "change-impact");
        assert_eq!(action_name(&TldrToolAction::Cfg), "cfg");
        assert_eq!(action_name(&TldrToolAction::Dfg), "dfg");
        assert_eq!(action_name(&TldrToolAction::Slice), "slice");
        assert_eq!(action_name(&TldrToolAction::Semantic), "semantic");
        assert_eq!(action_name(&TldrToolAction::Diagnostics), "diagnostics");
        assert_eq!(action_name(&TldrToolAction::Doctor), "doctor");
    }

    #[test]
    fn action_name_covers_daemon_actions() {
        assert_eq!(action_name(&TldrToolAction::Ping), "ping");
        assert_eq!(action_name(&TldrToolAction::Warm), "warm");
        assert_eq!(action_name(&TldrToolAction::Snapshot), "snapshot");
        assert_eq!(action_name(&TldrToolAction::Status), "status");
        assert_eq!(action_name(&TldrToolAction::Notify), "notify");
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_preserves_ping_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_hooks(
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
        .await
        .expect("ping tool should succeed");

        assert_eq!(result.text, "ping: pong");
        assert_eq!(result.structured_content["action"], "ping");
        assert_eq!(result.structured_content["status"], "ok");
        assert_eq!(result.structured_content["message"], "pong");
        assert_eq!(
            result.structured_content["structuredFailure"],
            serde_json::Value::Null
        );
        assert_eq!(
            result.structured_content["degradedMode"],
            serde_json::Value::Null
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_preserves_status_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_hooks(
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
        .await
        .expect("status tool should succeed");

        assert_eq!(result.text, "status: status");
        assert_eq!(result.structured_content["action"], "status");
        assert_eq!(result.structured_content["status"], "ok");
        assert_eq!(result.structured_content["message"], "status");
        assert_eq!(
            result.structured_content["structuredFailure"],
            serde_json::Value::Null
        );
        assert_eq!(
            result.structured_content["degradedMode"],
            serde_json::Value::Null
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_preserves_warm_snapshot_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_hooks(
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
                        snapshot: Some(crate::session::SessionSnapshot {
                            cached_entries: 0,
                            dirty_files: 0,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            background_reindex_in_progress: false,
                            last_query_at: None,
                            last_reindex: None,
                            last_reindex_attempt: None,
                            last_warm: None,
                            last_structured_failure: None,
                            degraded_mode_active: false,
                        }),
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("warm tool should succeed");

        assert_eq!(result.text, "warm: already warm");
        assert_eq!(result.structured_content["action"], "warm");
        assert_eq!(result.structured_content["snapshot"]["dirty_files"], 0);
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_preserves_snapshot_payload_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_hooks(
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
                        snapshot: Some(crate::session::SessionSnapshot {
                            cached_entries: 2,
                            dirty_files: 1,
                            dirty_file_threshold: 20,
                            reindex_pending: true,
                            background_reindex_in_progress: false,
                            last_query_at: None,
                            last_reindex: None,
                            last_reindex_attempt: None,
                            last_warm: None,
                            last_structured_failure: None,
                            degraded_mode_active: false,
                        }),
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("snapshot tool should succeed");

        assert_eq!(result.text, "snapshot: snapshot");
        assert_eq!(result.structured_content["action"], "snapshot");
        assert_eq!(result.structured_content["snapshot"]["cached_entries"], 2);
        assert_eq!(result.structured_content["snapshot"]["dirty_files"], 1);
    }

    #[test]
    fn structure_action_requires_language() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Structure,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
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
            ))
            .expect_err("structure without language should fail");

        assert_eq!(
            error.to_string(),
            "`language` is required for action=structure"
        );
    }

    #[test]
    fn extract_action_requires_path() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Extract,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
                    language: Some(TldrToolLanguage::Rust),
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
            ))
            .expect_err("extract without path should fail");

        assert_eq!(error.to_string(), "`path` is required for action=extract");
    }

    #[test]
    fn slice_action_requires_line() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Slice,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
                    language: Some(TldrToolLanguage::Rust),
                    symbol: Some("main".to_string()),
                    query: None,
                    module: None,
                    path: Some("src/lib.rs".to_string()),
                    line: None,
                    paths: None,
                    ..Default::default()
                },
                |_project_root, _command| Box::pin(async move { Ok(None) }),
                |_project_root| Box::pin(async move { Ok(false) }),
            ))
            .expect_err("slice without line should fail");

        assert_eq!(error.to_string(), "`line` is required for action=slice");
    }

    #[test]
    fn change_impact_action_requires_paths() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::ChangeImpact,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
                    language: Some(TldrToolLanguage::Rust),
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
            ))
            .expect_err("change-impact without paths should fail");

        assert_eq!(
            error.to_string(),
            "`paths` is required for action=change-impact"
        );
    }

    #[test]
    fn imports_action_requires_path() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Imports,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
                    language: Some(TldrToolLanguage::Rust),
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
            ))
            .expect_err("imports without path should fail");

        assert_eq!(error.to_string(), "`path` is required for action=imports");
    }

    #[test]
    fn importers_action_requires_module() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Importers,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
                    language: Some(TldrToolLanguage::Rust),
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
            ))
            .expect_err("importers without module should fail");

        assert_eq!(
            error.to_string(),
            "`module` is required for action=importers"
        );
    }

    #[test]
    fn semantic_action_requires_query() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Semantic,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
                    language: Some(TldrToolLanguage::Rust),
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
            ))
            .expect_err("semantic without query should fail");

        assert_eq!(error.to_string(), "`query` is required for action=semantic");
    }

    #[test]
    fn status_action_requires_daemon() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Status,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
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
            ))
            .expect_err("status without daemon should fail");

        assert!(
            error
                .to_string()
                .contains("native-tldr daemon is unavailable for"),
        );
        assert!(
            error
                .to_string()
                .contains("daemon unavailable (missing socket and pid)"),
        );
        assert!(error.to_string().contains("hint: run `codex tldr ...`"));
    }

    #[tokio::test]
    async fn query_daemon_with_hooks_detailed_preserves_ready_result_failure_metadata() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = query_daemon_with_hooks_detailed(
            tempdir.path(),
            &crate::daemon::TldrDaemonCommand::Status,
            &|_project_root, _command| Box::pin(async move { Ok(None) }),
            |_project_root| {
                Box::pin(async move {
                    Ok(DaemonReadyResult {
                        ready: false,
                        structured_failure: Some(StructuredFailure {
                            kind: StructuredFailureKind::DaemonUnavailable,
                            reason: "daemon boot timed out".to_string(),
                            retryable: true,
                            retry_hint: Some("start the daemon once".to_string()),
                        }),
                        degraded_mode: Some(DegradedMode {
                            kind: DegradedModeKind::DiagnosticOnly,
                            fallback_path: "status_only".to_string(),
                            reason: Some("daemon-only action".to_string()),
                        }),
                    })
                })
            },
        )
        .await
        .expect("detailed query should succeed");

        assert_eq!(result.response, None);
        assert_eq!(
            result.ready_result,
            Some(DaemonReadyResult {
                ready: false,
                structured_failure: Some(StructuredFailure {
                    kind: StructuredFailureKind::DaemonUnavailable,
                    reason: "daemon boot timed out".to_string(),
                    retryable: true,
                    retry_hint: Some("start the daemon once".to_string()),
                }),
                degraded_mode: Some(DegradedMode {
                    kind: DegradedModeKind::DiagnosticOnly,
                    fallback_path: "status_only".to_string(),
                    reason: Some("daemon-only action".to_string()),
                }),
            })
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_errors_when_daemon_analysis_payload_is_missing() {
        let tempdir = tempdir().expect("tempdir should exist");
        let error = run_tldr_tool_with_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Structure,
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
                        message: "missing".to_string(),
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
        .await
        .expect_err("missing analysis payload should fail");

        assert_eq!(
            error.to_string(),
            "daemon response missing analysis payload"
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_preserves_notify_text_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_hooks(
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
                        snapshot: None,
                        daemon_status: None,
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("notify tool should succeed");

        assert_eq!(result.text, "notify: marked src/lib.rs dirty");
        assert_eq!(result.structured_content["action"], "notify");
        assert_eq!(
            result.structured_content["message"],
            "marked src/lib.rs dirty"
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_preserves_status_details_contract() {
        let tempdir = tempdir().expect("tempdir should exist");
        let report = SemanticReindexReport {
            status: SemanticReindexStatus::Completed,
            languages: vec![SupportedLanguage::Rust],
            indexed_files: 2,
            indexed_units: 3,
            truncated: false,
            started_at: SystemTime::UNIX_EPOCH,
            finished_at: SystemTime::UNIX_EPOCH,
            message: "done".to_string(),
            embedding_enabled: true,
            embedding_dimensions: 256,
        };
        let result = run_tldr_tool_with_hooks(
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
                        snapshot: Some(crate::session::SessionSnapshot {
                            cached_entries: 1,
                            dirty_files: 0,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            background_reindex_in_progress: false,
                            last_query_at: Some(SystemTime::UNIX_EPOCH),
                            last_reindex: Some(report.clone()),
                            last_reindex_attempt: Some(report.clone()),
                            last_warm: Some(crate::session::WarmReport {
                                status: crate::session::WarmStatus::Loaded,
                                languages: vec![SupportedLanguage::Rust],
                                started_at: SystemTime::UNIX_EPOCH,
                                finished_at: SystemTime::UNIX_EPOCH,
                                message: "warm loaded 1 language indexes into daemon cache"
                                    .to_string(),
                            }),
                            last_structured_failure: None,
                            degraded_mode_active: false,
                        }),
                        daemon_status: Some(TldrDaemonStatus {
                            project_root: PathBuf::from("/tmp/project"),
                            socket_path: PathBuf::from("/tmp/project.sock"),
                            pid_path: PathBuf::from("/tmp/project.pid"),
                            lock_path: PathBuf::from("/tmp/project.lock"),
                            socket_exists: true,
                            pid_is_live: true,
                            lock_is_held: true,
                            healthy: true,
                            stale_socket: false,
                            stale_pid: false,
                            health_reason: None,
                            recovery_hint: None,
                            structured_failure: None,
                            degraded_mode: None,
                            semantic_reindex_pending: false,
                            semantic_reindex_in_progress: false,
                            last_query_at: Some(SystemTime::UNIX_EPOCH),
                            config: TldrDaemonConfigSummary {
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
        .await
        .expect("status tool should succeed");

        assert_eq!(result.structured_content["daemonStatus"]["healthy"], true);
        assert_eq!(
            result.structured_content["structuredFailure"],
            serde_json::Value::Null
        );
        assert_eq!(
            result.structured_content["degradedMode"],
            serde_json::Value::Null
        );
        assert_eq!(
            result.structured_content["daemonStatus"]["config"]["session_idle_timeout_secs"],
            1800
        );
        assert_eq!(
            result.structured_content["reindexReport"]["status"],
            "Completed"
        );
        assert_eq!(
            result.structured_content["snapshot"]["last_warm"]["status"],
            "Loaded"
        );
        assert_eq!(
            result.structured_content["snapshot"]["last_reindex"]["status"],
            "Completed"
        );
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_surfaces_structured_failure_for_unhealthy_status() {
        let tempdir = tempdir().expect("tempdir should exist");
        let result = run_tldr_tool_with_hooks(
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
                        daemon_status: Some(TldrDaemonStatus {
                            project_root: PathBuf::from("/tmp/project"),
                            socket_path: PathBuf::from("/tmp/project.sock"),
                            pid_path: PathBuf::from("/tmp/project.pid"),
                            lock_path: PathBuf::from("/tmp/project.lock"),
                            socket_exists: false,
                            pid_is_live: false,
                            lock_is_held: false,
                            healthy: false,
                            stale_socket: false,
                            stale_pid: false,
                            health_reason: Some("daemon missing".to_string()),
                            recovery_hint: Some("start the daemon".to_string()),
                            structured_failure: Some(StructuredFailure {
                                kind: StructuredFailureKind::DaemonUnavailable,
                                reason: "daemon missing".to_string(),
                                retryable: true,
                                retry_hint: Some("start the daemon".to_string()),
                            }),
                            degraded_mode: Some(DegradedMode {
                                kind: DegradedModeKind::DiagnosticOnly,
                                fallback_path: "status_only".to_string(),
                                reason: Some(
                                    "daemon-only actions cannot proceed without a live daemon"
                                        .to_string(),
                                ),
                            }),
                            semantic_reindex_pending: false,
                            semantic_reindex_in_progress: false,
                            last_query_at: None,
                            config: TldrDaemonConfigSummary {
                                auto_start: true,
                                socket_mode: "unix".to_string(),
                                semantic_enabled: true,
                                semantic_auto_reindex_threshold: 20,
                                session_dirty_file_threshold: 20,
                                session_idle_timeout_secs: 1800,
                            },
                        }),
                        reindex_report: None,
                    }))
                })
            },
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("status tool should succeed");

        assert_eq!(
            result.structured_content["structuredFailure"]["error_type"],
            "daemon_unavailable"
        );
        assert_eq!(
            result.structured_content["degradedMode"]["mode"],
            "diagnostic_only"
        );
        assert_eq!(
            result.structured_content["structuredFailure"]["retry_hint"],
            "start the daemon"
        );
    }

    #[test]
    fn notify_action_requires_path() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Notify,
                    project: Some(
                        tempdir()
                            .expect("tempdir should exist")
                            .path()
                            .display()
                            .to_string(),
                    ),
                    language: Some(TldrToolLanguage::Rust),
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
            ))
            .expect_err("notify without path should fail");

        assert_eq!(error.to_string(), "`path` is required for action=notify");
    }
}

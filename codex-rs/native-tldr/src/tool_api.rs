use crate::TldrEngine;
use crate::api::AnalysisKind;
use crate::api::AnalysisRequest;
use crate::daemon::TldrDaemonCommand;
use crate::daemon::TldrDaemonResponse;
use crate::daemon::daemon_health;
use crate::lang_support::LanguageRegistry;
use crate::lang_support::SupportedLanguage;
use crate::lifecycle::DaemonLifecycleManager;
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
    Tree,
    Context,
    Impact,
    Cfg,
    Dfg,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TldrToolResult {
    pub text: String,
    pub structured_content: serde_json::Value,
}

pub type QueryDaemonFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<TldrDaemonResponse>>> + Send + 'a>>;
pub type EnsureDaemonFuture<'a> = Pin<Box<dyn Future<Output = Result<bool>> + Send + 'a>>;

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
        TldrToolAction::Tree => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Tree,
                language,
                args.symbol,
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Context => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Context,
                language,
                args.symbol,
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
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Cfg => {
            let language = required_language(&args)?;
            run_analysis_tool(
                &project_root,
                TldrToolAction::Cfg,
                language,
                args.symbol,
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
                &query,
                &ensure_running,
            )
            .await
        }
        TldrToolAction::Semantic => {
            run_semantic_tool(&project_root, args, &query, &ensure_running).await
        }
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
    query: &Q,
    ensure_running: &E,
) -> Result<TldrToolResult>
where
    Q: for<'a> Fn(&'a Path, &'a TldrDaemonCommand) -> QueryDaemonFuture<'a>,
    E: for<'a> Fn(&'a Path) -> EnsureDaemonFuture<'a>,
{
    let kind = match action {
        TldrToolAction::Tree => AnalysisKind::Ast,
        TldrToolAction::Context => AnalysisKind::CallGraph,
        TldrToolAction::Impact => AnalysisKind::Pdg,
        TldrToolAction::Cfg => AnalysisKind::Cfg,
        TldrToolAction::Dfg => AnalysisKind::Dfg,
        _ => unreachable!("analysis action must map to AnalysisKind"),
    };
    let request = AnalysisRequest {
        kind,
        language,
        symbol: symbol.clone(),
    };
    let daemon_response = query_daemon_with_hooks(
        project_root,
        &TldrDaemonCommand::Analyze {
            key: analysis_cache_key(kind, language, symbol.as_deref()),
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
        "fallbackStrategy": support.fallback_strategy,
        "summary": analysis.summary,
        "symbol": symbol,
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
        return Err(anyhow::anyhow!(
            "native-tldr daemon is unavailable for {}",
            project_root.display()
        ));
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

pub async fn wait_for_external_daemon(project_root: &Path) -> Result<bool> {
    if !crate::daemon::daemon_lock_is_held(project_root)? {
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

pub fn action_name(action: &TldrToolAction) -> &'static str {
    match action {
        TldrToolAction::Tree => "tree",
        TldrToolAction::Context => "context",
        TldrToolAction::Impact => "impact",
        TldrToolAction::Cfg => "cfg",
        TldrToolAction::Dfg => "dfg",
        TldrToolAction::Semantic => "semantic",
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
) -> String {
    let symbol = symbol.unwrap_or("*");
    format!("{}:{kind:?}:{symbol}", language.as_str())
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

fn semantic_result_payload(payload: &serde_json::Value) -> serde_json::Value {
    let mut semantic = payload.clone();
    if let Some(object) = semantic.as_object_mut() {
        for key in ["action", "project", "language", "source"] {
            object.remove(key);
        }
    }
    semantic
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
            "semantic": {
              "type": "object",
              "properties": {
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
                }
              }
            },
            "analysisResult": {
              "type": "object",
              "properties": {
                "action": {
                  "enum": ["tree", "context", "impact", "cfg", "dfg"]
                },
                "project": { "type": "string" },
                "language": { "type": "string" },
                "source": { "type": "string" },
                "message": { "type": "string" },
                "supportLevel": { "type": "string" },
                "fallbackStrategy": { "type": "string" },
                "summary": { "type": "string" },
                "symbol": { "type": ["string", "null"] },
                "analysis": { "$ref": "#/$defs/analysis" }
              },
              "required": [
                "action",
                "project",
                "language",
                "source",
                "message",
                "supportLevel",
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
                "matches",
                "semantic"
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
                "snapshot": {
                  "type": "object",
                  "properties": {
                    "cached_entries": { "type": "integer" },
                    "dirty_files": { "type": "integer" },
                    "dirty_file_threshold": { "type": "integer" },
                    "reindex_pending": { "type": "boolean" },
                    "last_query_at": { "type": ["string", "null"] },
                    "last_reindex": { "$ref": "#/$defs/reindexReport" },
                    "last_reindex_attempt": { "$ref": "#/$defs/reindexReport" }
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
                    "semantic_reindex_pending": { "type": "boolean" },
                    "last_query_at": { "type": ["string", "null"] },
                    "config": {
                      "type": "object",
                      "properties": {
                        "auto_start": { "type": "boolean" },
                        "socket_mode": { "type": "string" },
                        "semantic_enabled": { "type": "boolean" },
                        "semantic_auto_reindex_threshold": { "type": "integer" },
                        "session_dirty_file_threshold": { "type": "integer" }
                      }
                    }
                  }
                },
                "reindexReport": { "$ref": "#/$defs/reindexReport" }
              },
              "required": ["action", "project", "status", "message"]
            }
          },
          "oneOf": [
            { "$ref": "#/$defs/analysisResult" },
            { "$ref": "#/$defs/semanticResult" },
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
    use super::run_tldr_tool_with_hooks;
    use crate::daemon::TldrDaemonConfigSummary;
    use crate::daemon::TldrDaemonResponse;
    use crate::daemon::TldrDaemonStatus;
    use crate::lang_support::SupportedLanguage;
    use crate::semantic::SemanticReindexReport;
    use crate::semantic::SemanticReindexStatus;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use std::time::SystemTime;
    use tempfile::tempdir;

    #[test]
    fn action_name_covers_analysis_actions() {
        assert_eq!(action_name(&TldrToolAction::Tree), "tree");
        assert_eq!(action_name(&TldrToolAction::Context), "context");
        assert_eq!(action_name(&TldrToolAction::Impact), "impact");
        assert_eq!(action_name(&TldrToolAction::Cfg), "cfg");
        assert_eq!(action_name(&TldrToolAction::Dfg), "dfg");
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
                path: None,
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "pong".to_string(),
                        analysis: None,
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
                path: None,
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "status".to_string(),
                        analysis: None,
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
                path: None,
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "already warm".to_string(),
                        analysis: None,
                        semantic: None,
                        snapshot: Some(crate::session::SessionSnapshot {
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
                path: None,
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "snapshot".to_string(),
                        analysis: None,
                        semantic: None,
                        snapshot: Some(crate::session::SessionSnapshot {
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
    fn tree_action_requires_language() {
        let error = tokio::runtime::Runtime::new()
            .expect("runtime should exist")
            .block_on(run_tldr_tool_with_hooks(
                TldrToolCallParam {
                    action: TldrToolAction::Tree,
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
                    path: None,
                },
                |_project_root, _command| Box::pin(async move { Ok(None) }),
                |_project_root| Box::pin(async move { Ok(false) }),
            ))
            .expect_err("tree without language should fail");

        assert_eq!(error.to_string(), "`language` is required for action=tree");
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
                    path: None,
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
                    path: None,
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
    }

    #[tokio::test]
    async fn run_tldr_tool_with_hooks_errors_when_daemon_analysis_payload_is_missing() {
        let tempdir = tempdir().expect("tempdir should exist");
        let error = run_tldr_tool_with_hooks(
            TldrToolCallParam {
                action: TldrToolAction::Tree,
                project: Some(tempdir.path().display().to_string()),
                language: Some(TldrToolLanguage::Rust),
                symbol: None,
                query: None,
                path: None,
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "missing".to_string(),
                        analysis: None,
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
                path: Some("src/lib.rs".to_string()),
            },
            |_project_root, _command| {
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "marked src/lib.rs dirty".to_string(),
                        analysis: None,
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
                path: None,
            },
            |_project_root, _command| {
                let report = report.clone();
                Box::pin(async move {
                    Ok(Some(TldrDaemonResponse {
                        status: "ok".to_string(),
                        message: "status".to_string(),
                        analysis: None,
                        semantic: None,
                        snapshot: Some(crate::session::SessionSnapshot {
                            cached_entries: 1,
                            dirty_files: 0,
                            dirty_file_threshold: 20,
                            reindex_pending: false,
                            last_query_at: Some(SystemTime::UNIX_EPOCH),
                            last_reindex: Some(report.clone()),
                            last_reindex_attempt: Some(report.clone()),
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
                            semantic_reindex_pending: false,
                            last_query_at: Some(SystemTime::UNIX_EPOCH),
                            config: TldrDaemonConfigSummary {
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
            |_project_root| Box::pin(async move { Ok(false) }),
        )
        .await
        .expect("status tool should succeed");

        assert_eq!(result.structured_content["daemonStatus"]["healthy"], true);
        assert_eq!(
            result.structured_content["reindexReport"]["status"],
            "Completed"
        );
        assert_eq!(
            result.structured_content["snapshot"]["last_reindex"]["status"],
            "Completed"
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
                    path: None,
                },
                |_project_root, _command| Box::pin(async move { Ok(None) }),
                |_project_root| Box::pin(async move { Ok(false) }),
            ))
            .expect_err("notify without path should fail");

        assert_eq!(error.to_string(), "`path` is required for action=notify");
    }
}

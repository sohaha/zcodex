use anyhow::Result;
use codex_native_tldr::TldrEngine;
use codex_native_tldr::api::AnalysisKind;
use codex_native_tldr::api::AnalysisRequest;
use codex_native_tldr::daemon::TldrDaemonCommand;
use codex_native_tldr::daemon::query_daemon;
use codex_native_tldr::lang_support::LanguageRegistry;
use codex_native_tldr::lang_support::SupportedLanguage;
use rmcp::model::CallToolResult;
use rmcp::model::Content;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use schemars::JsonSchema;
use schemars::r#gen::SchemaSettings;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

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
    let daemon_response = query_daemon(
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
        let engine = TldrEngine::builder(project_root.clone()).build();
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
    let engine = TldrEngine::builder(project_root.clone()).build();
    let structured_content = json!({
        "action": "semantic",
        "project": project_root,
        "language": language.as_str(),
        "query": query,
        "enabled": engine.config().semantic.enabled,
        "source": "local",
        "message": "semantic search is not enabled in this build yet",
    });
    Ok(success_result(
        format!(
            "semantic {} enabled={}: semantic search is not enabled in this build yet",
            language.as_str(),
            engine.config().semantic.enabled
        ),
        structured_content,
    ))
}

async fn run_daemon_tool(
    project_root: PathBuf,
    command: TldrDaemonCommand,
    action: &str,
) -> Result<CallToolResult> {
    let Some(response) = query_daemon(&project_root, &command).await? else {
        return Err(anyhow::anyhow!(
            "native-tldr daemon is unavailable for {}",
            project_root.display()
        ));
    };
    let structured_content = json!({
        "action": action,
        "project": project_root,
        "status": response.status,
        "message": response.message,
        "snapshot": response.snapshot,
    });
    let text = structured_content
        .get("message")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("ok")
        .to_string();
    Ok(success_result(text, structured_content))
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
            "summary": { "type": "string" }
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
    use pretty_assertions::assert_eq;

    #[test]
    fn verify_tldr_tool_json_schema() {
        let tool = create_tool_for_tldr_tool_call_param();
        let tool_json = serde_json::to_value(&tool).expect("tool serializes");
        let expected_tool_json = serde_json::json!({
          "description": "Structured code context analysis via native-tldr with daemon-first execution.",
          "inputSchema": {
            "properties": {
              "action": {
                "enum": ["tree", "context", "impact", "semantic", "ping", "warm", "snapshot", "notify"],
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
}

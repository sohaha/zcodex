use crate::config::ZmemoryConfig;
use crate::path_resolution::resolve_workspace_base_path;
use crate::path_resolution::resolve_zmemory_path;
use crate::schema::DEFAULT_DOMAIN;
use crate::service::execute_action;
use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ZmemoryToolAction {
    Read,
    Search,
    Create,
    Update,
    DeletePath,
    AddAlias,
    ManageTriggers,
    Stats,
    Doctor,
    RebuildSearch,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ZmemoryToolCallParam {
    pub action: ZmemoryToolAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub codex_home: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_string: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_string: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disclosure: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub add: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remove: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

impl Default for ZmemoryToolCallParam {
    fn default() -> Self {
        Self {
            action: ZmemoryToolAction::Stats,
            codex_home: None,
            uri: None,
            parent_uri: None,
            new_uri: None,
            target_uri: None,
            query: None,
            content: None,
            title: None,
            old_string: None,
            new_string: None,
            append: None,
            priority: None,
            disclosure: None,
            add: None,
            remove: None,
            limit: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZmemoryToolResult {
    pub text: String,
    pub structured_content: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ZmemoryUri {
    pub domain: String,
    pub path: String,
}

impl ZmemoryUri {
    pub(crate) fn parse(raw: &str) -> Result<Self> {
        let trimmed = raw.trim();
        anyhow::ensure!(!trimmed.is_empty(), "uri cannot be empty");
        if let Some((domain, path)) = trimmed.split_once("://") {
            let domain = domain.trim().to_lowercase();
            anyhow::ensure!(!domain.is_empty(), "uri domain cannot be empty");
            return Ok(Self {
                domain,
                path: normalize_path(path),
            });
        }
        Ok(Self {
            domain: DEFAULT_DOMAIN.to_string(),
            path: normalize_path(trimmed),
        })
    }

    pub(crate) fn is_root(&self) -> bool {
        self.path.is_empty()
    }

    pub(crate) fn parent(&self) -> Self {
        if self.path.is_empty() {
            return self.clone();
        }
        match self.path.rsplit_once('/') {
            Some((parent, _)) => Self {
                domain: self.domain.clone(),
                path: parent.to_string(),
            },
            None => Self {
                domain: self.domain.clone(),
                path: String::new(),
            },
        }
    }

    pub(crate) fn leaf_name(&self) -> Result<&str> {
        anyhow::ensure!(!self.path.is_empty(), "root path has no leaf name");
        Ok(self.path.rsplit('/').next().unwrap_or_default())
    }
}

impl fmt::Display for ZmemoryUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.domain, self.path)
    }
}

pub fn run_zmemory_tool(
    codex_home: &Path,
    args: ZmemoryToolCallParam,
) -> Result<ZmemoryToolResult> {
    let cwd = std::env::current_dir()?;
    run_zmemory_tool_with_context(codex_home, &cwd, None, args)
}

pub fn run_zmemory_tool_with_context(
    codex_home: &Path,
    cwd: &Path,
    zmemory_path: Option<&Path>,
    args: ZmemoryToolCallParam,
) -> Result<ZmemoryToolResult> {
    let path_resolution = resolve_zmemory_path(codex_home, cwd, zmemory_path)?;
    let workspace_base = resolve_workspace_base_path(cwd)?;
    let config = ZmemoryConfig::new(codex_home, workspace_base, path_resolution);
    let structured_content = execute_action(&config, &args)?;
    let text = render_summary(&structured_content);
    Ok(ZmemoryToolResult {
        text,
        structured_content,
    })
}

pub fn zmemory_tool_output_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "action": { "type": "string" },
            "result": { "type": "object" }
        },
        "required": ["action", "result"],
        "additionalProperties": true
    })
}

fn normalize_path(raw: &str) -> String {
    raw.trim().trim_matches('/').to_string()
}

fn render_summary(payload: &serde_json::Value) -> String {
    let action = payload
        .get("action")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("unknown");
    let result = payload.get("result").unwrap_or(&serde_json::Value::Null);
    match action {
        "read" => {
            let uri = result
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            if let Some(view_name) = result
                .get("view")
                .and_then(|view| view.get("view"))
                .and_then(serde_json::Value::as_str)
            {
                format!("read {uri}: {view_name} view")
            } else {
                format!(
                    "read {uri}: {} children, {} keywords",
                    result
                        .get("children")
                        .and_then(serde_json::Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default(),
                    result
                        .get("keywords")
                        .and_then(serde_json::Value::as_array)
                        .map(Vec::len)
                        .unwrap_or_default()
                )
            }
        }
        "search" => format!(
            "search {}: {} matches",
            result
                .get("query")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default(),
            result
                .get("matches")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or_default()
        ),
        "create" => format!(
            "created {}",
            result
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
        ),
        "update" => format!(
            "updated {}",
            result
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
        ),
        "delete-path" => format!(
            "deleted {}",
            result
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
        ),
        "add-alias" => format!(
            "alias {} -> {}",
            result
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown"),
            result
                .get("targetUri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
        ),
        "manage-triggers" => format!(
            "updated triggers for {}",
            result
                .get("uri")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown")
        ),
        "stats" => format!(
            "stats: {} nodes, {} paths, {} docs ({}, {})",
            result
                .get("nodeCount")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default(),
            result
                .get("pathCount")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default(),
            result
                .get("searchDocumentCount")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default(),
            result
                .get("dbPath")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown db"),
            result
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown reason")
        ),
        "doctor" => format!(
            "doctor: {} ({}, {})",
            if result
                .get("healthy")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                "healthy"
            } else {
                "unhealthy"
            },
            result
                .get("dbPath")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown db"),
            result
                .get("reason")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown reason")
        ),
        "rebuild-search" => format!(
            "rebuilt search index: {} documents",
            result
                .get("documentCount")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or_default()
        ),
        _ => "zmemory result".to_string(),
    }
}

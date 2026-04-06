use crate::config::ZmemoryConfig;
use crate::config::ZmemorySettings;
use crate::path_resolution::resolve_workspace_base_path;
use crate::path_resolution::resolve_zmemory_path;
use crate::schema::DEFAULT_DOMAIN;
use crate::service::execute_action;
use anyhow::Result;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Deserializer;
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
    Audit,
    Doctor,
    RebuildSearch,
}

#[derive(Debug, Clone, Serialize, JsonSchema, PartialEq, Eq)]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_action: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommonToolArgs {
    #[serde(default, alias = "codexHome")]
    codex_home: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
enum TaggedZmemoryToolCallParam {
    Read {
        #[serde(flatten)]
        common: CommonToolArgs,
        uri: String,
        limit: Option<usize>,
    },
    Search {
        #[serde(flatten)]
        common: CommonToolArgs,
        query: String,
        uri: Option<String>,
        limit: Option<usize>,
    },
    Create {
        #[serde(flatten)]
        common: CommonToolArgs,
        uri: Option<String>,
        #[serde(alias = "parentUri")]
        parent_uri: Option<String>,
        content: String,
        title: Option<String>,
        priority: Option<i64>,
        disclosure: Option<String>,
    },
    Update {
        #[serde(flatten)]
        common: CommonToolArgs,
        uri: String,
        content: Option<String>,
        #[serde(alias = "oldString")]
        old_string: Option<String>,
        #[serde(alias = "newString")]
        new_string: Option<String>,
        append: Option<String>,
        priority: Option<i64>,
        disclosure: Option<String>,
    },
    DeletePath {
        #[serde(flatten)]
        common: CommonToolArgs,
        uri: String,
    },
    AddAlias {
        #[serde(flatten)]
        common: CommonToolArgs,
        #[serde(alias = "newUri")]
        new_uri: String,
        #[serde(alias = "targetUri")]
        target_uri: String,
        priority: Option<i64>,
        disclosure: Option<String>,
    },
    ManageTriggers {
        #[serde(flatten)]
        common: CommonToolArgs,
        uri: String,
        add: Option<Vec<String>>,
        remove: Option<Vec<String>>,
    },
    Stats {
        #[serde(flatten)]
        common: CommonToolArgs,
    },
    Audit {
        #[serde(flatten)]
        common: CommonToolArgs,
        limit: Option<usize>,
        #[serde(alias = "auditAction")]
        audit_action: Option<String>,
        uri: Option<String>,
    },
    Doctor {
        #[serde(flatten)]
        common: CommonToolArgs,
    },
    RebuildSearch {
        #[serde(flatten)]
        common: CommonToolArgs,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyZmemoryToolCallParam {
    action: ZmemoryToolAction,
    #[serde(default, alias = "codex_home")]
    codex_home: Option<String>,
    #[serde(default)]
    uri: Option<String>,
    #[serde(default, alias = "parent_uri")]
    parent_uri: Option<String>,
    #[serde(default, alias = "new_uri")]
    new_uri: Option<String>,
    #[serde(default, alias = "target_uri")]
    target_uri: Option<String>,
    #[serde(default)]
    query: Option<String>,
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default, alias = "old_string")]
    old_string: Option<String>,
    #[serde(default, alias = "new_string")]
    new_string: Option<String>,
    #[serde(default)]
    append: Option<String>,
    #[serde(default)]
    priority: Option<i64>,
    #[serde(default)]
    disclosure: Option<String>,
    #[serde(default)]
    add: Option<Vec<String>>,
    #[serde(default)]
    remove: Option<Vec<String>>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default, alias = "auditAction")]
    audit_action: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ZmemoryToolCallParamRepr {
    Tagged(TaggedZmemoryToolCallParam),
    Legacy(LegacyZmemoryToolCallParam),
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
            audit_action: None,
        }
    }
}

impl<'de> Deserialize<'de> for ZmemoryToolCallParam {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match ZmemoryToolCallParamRepr::deserialize(deserializer)? {
            ZmemoryToolCallParamRepr::Tagged(tagged) => Ok(tagged.into()),
            ZmemoryToolCallParamRepr::Legacy(legacy) => Ok(legacy.into()),
        }
    }
}

impl From<LegacyZmemoryToolCallParam> for ZmemoryToolCallParam {
    fn from(legacy: LegacyZmemoryToolCallParam) -> Self {
        Self {
            action: legacy.action,
            codex_home: legacy.codex_home,
            uri: legacy.uri,
            parent_uri: legacy.parent_uri,
            new_uri: legacy.new_uri,
            target_uri: legacy.target_uri,
            query: legacy.query,
            content: legacy.content,
            title: legacy.title,
            old_string: legacy.old_string,
            new_string: legacy.new_string,
            append: legacy.append,
            priority: legacy.priority,
            disclosure: legacy.disclosure,
            add: legacy.add,
            remove: legacy.remove,
            limit: legacy.limit,
            audit_action: legacy.audit_action,
        }
    }
}

impl From<TaggedZmemoryToolCallParam> for ZmemoryToolCallParam {
    fn from(tagged: TaggedZmemoryToolCallParam) -> Self {
        match tagged {
            TaggedZmemoryToolCallParam::Read { common, uri, limit } => Self {
                action: ZmemoryToolAction::Read,
                codex_home: common.codex_home,
                uri: Some(uri),
                limit,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::Search {
                common,
                query,
                uri,
                limit,
            } => Self {
                action: ZmemoryToolAction::Search,
                codex_home: common.codex_home,
                query: Some(query),
                uri,
                limit,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::Create {
                common,
                uri,
                parent_uri,
                content,
                title,
                priority,
                disclosure,
            } => Self {
                action: ZmemoryToolAction::Create,
                codex_home: common.codex_home,
                uri,
                parent_uri,
                content: Some(content),
                title,
                priority,
                disclosure,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::Update {
                common,
                uri,
                content,
                old_string,
                new_string,
                append,
                priority,
                disclosure,
            } => Self {
                action: ZmemoryToolAction::Update,
                codex_home: common.codex_home,
                uri: Some(uri),
                content,
                old_string,
                new_string,
                append,
                priority,
                disclosure,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::DeletePath { common, uri } => Self {
                action: ZmemoryToolAction::DeletePath,
                codex_home: common.codex_home,
                uri: Some(uri),
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::AddAlias {
                common,
                new_uri,
                target_uri,
                priority,
                disclosure,
            } => Self {
                action: ZmemoryToolAction::AddAlias,
                codex_home: common.codex_home,
                new_uri: Some(new_uri),
                target_uri: Some(target_uri),
                priority,
                disclosure,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::ManageTriggers {
                common,
                uri,
                add,
                remove,
            } => Self {
                action: ZmemoryToolAction::ManageTriggers,
                codex_home: common.codex_home,
                uri: Some(uri),
                add,
                remove,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::Stats { common } => Self {
                action: ZmemoryToolAction::Stats,
                codex_home: common.codex_home,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::Audit {
                common,
                limit,
                audit_action,
                uri,
            } => Self {
                action: ZmemoryToolAction::Audit,
                codex_home: common.codex_home,
                limit,
                audit_action,
                uri,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::Doctor { common } => Self {
                action: ZmemoryToolAction::Doctor,
                codex_home: common.codex_home,
                ..Self::default()
            },
            TaggedZmemoryToolCallParam::RebuildSearch { common } => Self {
                action: ZmemoryToolAction::RebuildSearch,
                codex_home: common.codex_home,
                ..Self::default()
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ZmemoryActionInput {
    Read(ReadActionParams),
    Search(SearchActionParams),
    Create(CreateActionParams),
    Update(UpdateActionParams),
    DeletePath(UriActionParams),
    AddAlias(AddAliasActionParams),
    ManageTriggers(ManageTriggersActionParams),
    Stats,
    Audit(AuditActionParams),
    Doctor,
    RebuildSearch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadActionParams {
    pub(crate) uri: ZmemoryUri,
    pub(crate) limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SearchActionParams {
    pub(crate) query: String,
    pub(crate) uri: Option<ZmemoryUri>,
    pub(crate) limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreateActionParams {
    pub(crate) uri: Option<ZmemoryUri>,
    pub(crate) parent_uri: Option<ZmemoryUri>,
    pub(crate) content: String,
    pub(crate) title: Option<String>,
    pub(crate) priority: i64,
    pub(crate) disclosure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UpdateActionParams {
    pub(crate) uri: ZmemoryUri,
    pub(crate) content: Option<String>,
    pub(crate) old_string: Option<String>,
    pub(crate) new_string: Option<String>,
    pub(crate) append: Option<String>,
    pub(crate) priority: Option<i64>,
    pub(crate) disclosure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UriActionParams {
    pub(crate) uri: ZmemoryUri,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AddAliasActionParams {
    pub(crate) new_uri: ZmemoryUri,
    pub(crate) target_uri: ZmemoryUri,
    pub(crate) priority: Option<i64>,
    pub(crate) disclosure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ManageTriggersActionParams {
    pub(crate) uri: ZmemoryUri,
    pub(crate) add: Vec<String>,
    pub(crate) remove: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AuditActionParams {
    pub(crate) limit: usize,
    pub(crate) audit_action: Option<String>,
    pub(crate) uri: Option<ZmemoryUri>,
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

impl TryFrom<&ZmemoryToolCallParam> for ZmemoryActionInput {
    type Error = anyhow::Error;

    fn try_from(args: &ZmemoryToolCallParam) -> Result<Self> {
        match args.action {
            ZmemoryToolAction::Read => Ok(Self::Read(ReadActionParams {
                uri: parse_required_uri(args.uri.as_deref())?,
                limit: args.limit.unwrap_or(20),
            })),
            ZmemoryToolAction::Search => Ok(Self::Search(SearchActionParams {
                query: required_query(args.query.as_deref())?,
                uri: args.uri.as_deref().map(ZmemoryUri::parse).transpose()?,
                limit: args.limit.unwrap_or(10),
            })),
            ZmemoryToolAction::Create => Ok(Self::Create(CreateActionParams {
                uri: args.uri.as_deref().map(ZmemoryUri::parse).transpose()?,
                parent_uri: args
                    .parent_uri
                    .as_deref()
                    .map(ZmemoryUri::parse)
                    .transpose()?,
                content: required_content(args.content.as_deref())?,
                title: normalize_optional_text(args.title.as_deref()),
                priority: args.priority.unwrap_or_default(),
                disclosure: normalize_optional_text(args.disclosure.as_deref()),
            })),
            ZmemoryToolAction::Update => Ok(Self::Update(UpdateActionParams {
                uri: parse_required_uri(args.uri.as_deref())?,
                content: args.content.clone(),
                old_string: args.old_string.clone(),
                new_string: args.new_string.clone(),
                append: args.append.clone(),
                priority: args.priority,
                disclosure: args.disclosure.clone(),
            })),
            ZmemoryToolAction::DeletePath => Ok(Self::DeletePath(UriActionParams {
                uri: parse_required_uri(args.uri.as_deref())?,
            })),
            ZmemoryToolAction::AddAlias => Ok(Self::AddAlias(AddAliasActionParams {
                new_uri: parse_required_uri(args.new_uri.as_deref())?,
                target_uri: parse_required_uri(args.target_uri.as_deref())?,
                priority: args.priority,
                disclosure: args.disclosure.clone(),
            })),
            ZmemoryToolAction::ManageTriggers => {
                Ok(Self::ManageTriggers(ManageTriggersActionParams {
                    uri: parse_required_uri(args.uri.as_deref())?,
                    add: args.add.clone().unwrap_or_default(),
                    remove: args.remove.clone().unwrap_or_default(),
                }))
            }
            ZmemoryToolAction::Stats => Ok(Self::Stats),
            ZmemoryToolAction::Audit => Ok(Self::Audit(AuditActionParams {
                limit: args.limit.unwrap_or(20),
                audit_action: normalize_optional_text(args.audit_action.as_deref())
                    .map(|value| value.to_lowercase()),
                uri: args.uri.as_deref().map(ZmemoryUri::parse).transpose()?,
            })),
            ZmemoryToolAction::Doctor => Ok(Self::Doctor),
            ZmemoryToolAction::RebuildSearch => Ok(Self::RebuildSearch),
        }
    }
}

fn parse_required_uri(raw: Option<&str>) -> Result<ZmemoryUri> {
    let raw = raw.ok_or_else(|| anyhow::anyhow!("`uri` is required"))?;
    ZmemoryUri::parse(raw)
}

fn required_query(query: Option<&str>) -> Result<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("`query` is required for action=search"))
}

fn required_content(content: Option<&str>) -> Result<String> {
    content
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("`content` is required"))
}

fn normalize_optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

pub fn run_zmemory_tool(
    codex_home: &Path,
    args: ZmemoryToolCallParam,
) -> Result<ZmemoryToolResult> {
    let cwd = std::env::current_dir()?;
    run_zmemory_tool_with_context(codex_home, &cwd, None, None, args)
}

pub fn run_zmemory_tool_with_context(
    codex_home: &Path,
    cwd: &Path,
    zmemory_path: Option<&Path>,
    settings: Option<ZmemorySettings>,
    args: ZmemoryToolCallParam,
) -> Result<ZmemoryToolResult> {
    let path_resolution = resolve_zmemory_path(codex_home, cwd, zmemory_path)?;
    let workspace_base = resolve_workspace_base_path(cwd)?;
    let config = match settings {
        Some(settings) => {
            ZmemoryConfig::new_with_settings(codex_home, &workspace_base, path_resolution, settings)
        }
        None => ZmemoryConfig::new(codex_home, &workspace_base, path_resolution),
    };
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
        "audit" => format!(
            "audit: {} entries",
            result
                .get("entries")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or_default()
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

#[cfg(test)]
mod tests {
    use super::ZmemoryToolAction;
    use super::ZmemoryToolCallParam;
    use pretty_assertions::assert_eq;

    #[test]
    fn deserialize_tagged_union_uses_snake_case_fields() {
        let args: ZmemoryToolCallParam = serde_json::from_value(serde_json::json!({
            "action": "add-alias",
            "codex_home": "/tmp/codex",
            "new_uri": "core://alias",
            "target_uri": "core://target",
            "priority": 3
        }))
        .expect("deserialize tagged args");

        assert_eq!(
            args,
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::AddAlias,
                codex_home: Some("/tmp/codex".to_string()),
                new_uri: Some("core://alias".to_string()),
                target_uri: Some("core://target".to_string()),
                priority: Some(3),
                ..ZmemoryToolCallParam::default()
            }
        );
    }

    #[test]
    fn deserialize_tagged_union_accepts_camel_case_aliases() {
        let args: ZmemoryToolCallParam = serde_json::from_value(serde_json::json!({
            "action": "update",
            "codexHome": "/tmp/codex",
            "uri": "core://agent",
            "oldString": "before",
            "newString": "after"
        }))
        .expect("deserialize tagged camelCase args");

        assert_eq!(
            args,
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Update,
                codex_home: Some("/tmp/codex".to_string()),
                uri: Some("core://agent".to_string()),
                old_string: Some("before".to_string()),
                new_string: Some("after".to_string()),
                ..ZmemoryToolCallParam::default()
            }
        );
    }

    #[test]
    fn deserialize_tagged_union_accepts_audit_limit() {
        let args: ZmemoryToolCallParam = serde_json::from_value(serde_json::json!({
            "action": "audit",
            "limit": 5,
            "auditAction": "add-alias",
            "uri": "core://alias"
        }))
        .expect("deserialize audit args");

        assert_eq!(
            args,
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Audit,
                limit: Some(5),
                audit_action: Some("add-alias".to_string()),
                uri: Some("core://alias".to_string()),
                ..ZmemoryToolCallParam::default()
            }
        );
    }

    #[test]
    fn deserialize_legacy_flat_shape_still_works() {
        let args: ZmemoryToolCallParam = serde_json::from_value(serde_json::json!({
            "action": "search",
            "query": "hello",
            "limit": 5
        }))
        .expect("deserialize legacy args");

        assert_eq!(
            args,
            ZmemoryToolCallParam {
                action: ZmemoryToolAction::Search,
                query: Some("hello".to_string()),
                limit: Some(5),
                ..ZmemoryToolCallParam::default()
            }
        );
    }
}

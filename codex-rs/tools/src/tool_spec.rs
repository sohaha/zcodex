use crate::FreeformTool;
use crate::JsonSchema;
use crate::ResponsesApiTool;
use codex_native_tldr::tool_api::tldr_tool_output_schema;
use codex_protocol::config_types::WebSearchConfig;
use codex_protocol::config_types::WebSearchContextSize;
use codex_protocol::config_types::WebSearchFilters as ConfigWebSearchFilters;
use codex_protocol::config_types::WebSearchMode;
use codex_protocol::config_types::WebSearchUserLocation as ConfigWebSearchUserLocation;
use codex_protocol::config_types::WebSearchUserLocationType;
use codex_protocol::openai_models::WebSearchToolType;
use serde::Serialize;
use serde_json::Map as JsonMap;
use serde_json::Value;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

const WEB_SEARCH_TEXT_AND_IMAGE_CONTENT_TYPES: [&str; 2] = ["text", "image"];

/// When serialized as JSON, this produces a valid "Tool" in the OpenAI
/// Responses API.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type")]
pub enum ToolSpec {
    #[serde(rename = "function")]
    Function(ResponsesApiTool),
    #[serde(rename = "tool_search")]
    ToolSearch {
        execution: String,
        description: String,
        parameters: JsonSchema,
    },
    #[serde(rename = "local_shell")]
    LocalShell {},
    #[serde(rename = "image_generation")]
    ImageGeneration { output_format: String },
    // TODO: Understand why we get an error on web_search although the API docs
    // say it's supported.
    // https://platform.openai.com/docs/guides/tools-web-search?api-mode=responses#:~:text=%7B%20type%3A%20%22web_search%22%20%7D%2C
    // The `external_web_access` field determines whether the web search is over
    // cached or live content.
    // https://platform.openai.com/docs/guides/tools-web-search#live-internet-access
    #[serde(rename = "web_search")]
    WebSearch {
        #[serde(skip_serializing_if = "Option::is_none")]
        external_web_access: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filters: Option<ResponsesApiWebSearchFilters>,
        #[serde(skip_serializing_if = "Option::is_none")]
        user_location: Option<ResponsesApiWebSearchUserLocation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_context_size: Option<WebSearchContextSize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        search_content_types: Option<Vec<String>>,
    },
    #[serde(rename = "custom")]
    Freeform(FreeformTool),
}

impl ToolSpec {
    pub fn name(&self) -> &str {
        match self {
            ToolSpec::Function(tool) => tool.name.as_str(),
            ToolSpec::ToolSearch { .. } => "tool_search",
            ToolSpec::LocalShell {} => "local_shell",
            ToolSpec::ImageGeneration { .. } => "image_generation",
            ToolSpec::WebSearch { .. } => "web_search",
            ToolSpec::Freeform(tool) => tool.name.as_str(),
        }
    }
}

pub fn create_local_shell_tool() -> ToolSpec {
    ToolSpec::LocalShell {}
}

pub fn create_tldr_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "action".to_string(),
            JsonSchema::String {
                description: Some(
                    "Action to run. Analysis/search: structure, search, extract, imports, importers, context, impact, calls, dead, arch, change-impact, cfg, dfg, slice, semantic, diagnostics, doctor. Daemon: ping, warm, snapshot, status, notify."
                        .to_string(),
                ),
            },
        ),
        (
            "project".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional project root. Defaults to the current session working directory."
                        .to_string(),
                ),
            },
        ),
        (
            "language".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional language. Supported: rust, c, cpp, csharp, elixir, java, kotlin, typescript, javascript, lua, luau, python, go, php, ruby, scala, swift, zig."
                        .to_string(),
                ),
            },
        ),
        (
            "symbol".to_string(),
            JsonSchema::String {
                description: Some(
                    "Optional symbol for structure/context/impact/calls/dead/arch/cfg/dfg/slice."
                        .to_string(),
                ),
            },
        ),
        (
            "query".to_string(),
            JsonSchema::String {
                description: Some("Query text for action=search or action=semantic.".to_string()),
            },
        ),
        (
            "path".to_string(),
            JsonSchema::String {
                description: Some(
                    "Path for action=extract/imports/slice, or changed path for action=notify."
                        .to_string(),
                ),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "ztldr".to_string(),
        description: "Use ztldr first for structural code understanding (symbols, calls, impact, semantic code search) before broad grep/read. Prefer raw grep/read for regex or exact text checks. If output includes degradedMode or structuredFailure, report it explicitly."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["action".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: Some(tldr_tool_output_schema()),
    })
}

pub fn create_image_generation_tool(output_format: &str) -> ToolSpec {
    ToolSpec::ImageGeneration {
        output_format: output_format.to_string(),
    }
}

pub struct WebSearchToolOptions<'a> {
    pub web_search_mode: Option<WebSearchMode>,
    pub web_search_config: Option<&'a WebSearchConfig>,
    pub web_search_tool_type: WebSearchToolType,
}

pub fn create_web_search_tool(options: WebSearchToolOptions<'_>) -> Option<ToolSpec> {
    let external_web_access = match options.web_search_mode {
        Some(WebSearchMode::Cached) => Some(false),
        Some(WebSearchMode::Live) => Some(true),
        Some(WebSearchMode::Disabled) | None => None,
    }?;

    let search_content_types = match options.web_search_tool_type {
        WebSearchToolType::Text => None,
        WebSearchToolType::TextAndImage => Some(
            WEB_SEARCH_TEXT_AND_IMAGE_CONTENT_TYPES
                .into_iter()
                .map(str::to_string)
                .collect(),
        ),
    };

    Some(ToolSpec::WebSearch {
        external_web_access: Some(external_web_access),
        filters: options
            .web_search_config
            .and_then(|config| config.filters.clone().map(Into::into)),
        user_location: options
            .web_search_config
            .and_then(|config| config.user_location.clone().map(Into::into)),
        search_context_size: options
            .web_search_config
            .and_then(|config| config.search_context_size),
        search_content_types,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConfiguredToolSpec {
    pub spec: ToolSpec,
    pub supports_parallel_tool_calls: bool,
}

impl ConfiguredToolSpec {
    pub fn new(spec: ToolSpec, supports_parallel_tool_calls: bool) -> Self {
        Self {
            spec,
            supports_parallel_tool_calls,
        }
    }

    pub fn name(&self) -> &str {
        self.spec.name()
    }
}

/// Returns JSON values that are compatible with Function Calling in the
/// Responses API:
/// https://platform.openai.com/docs/guides/function-calling?api-mode=responses
pub fn create_tools_json_for_responses_api(
    tools: &[ToolSpec],
) -> Result<Vec<Value>, serde_json::Error> {
    let mut tools_json = Vec::new();

    for tool in tools {
        let json = responses_api_tool_json(tool)?;
        tools_json.push(json);
    }

    Ok(tools_json)
}

fn responses_api_tool_json(tool: &ToolSpec) -> Result<Value, serde_json::Error> {
    match tool {
        ToolSpec::Function(ResponsesApiTool {
            parameters: JsonSchema::OneOf { .. },
            ..
        }) => {
            let mut json = serde_json::to_value(tool)?;
            if let Some(parameters) = json.get_mut("parameters") {
                *parameters = normalize_top_level_schema(parameters.take());
            }
            Ok(json)
        }
        _ => serde_json::to_value(tool),
    }
}

fn normalize_top_level_schema(schema: Value) -> Value {
    let Some(variants) = schema.get("oneOf").and_then(Value::as_array) else {
        return schema;
    };
    collapse_top_level_one_of_to_object(variants).unwrap_or(schema)
}

fn collapse_top_level_one_of_to_object(variants: &[Value]) -> Option<Value> {
    let mut properties = JsonMap::new();
    let mut required_intersection: Option<BTreeSet<String>> = None;
    let mut disallow_additional_properties = true;

    for variant in variants {
        let object = variant.as_object()?;
        if object.get("type")?.as_str()? != "object" {
            return None;
        }

        let variant_properties = object.get("properties")?.as_object()?;
        for (key, value) in variant_properties {
            merge_property_schema(&mut properties, key, value.clone());
        }

        let required = object
            .get("required")
            .and_then(Value::as_array)
            .map(|values| {
                values
                    .iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();

        required_intersection = Some(match required_intersection.take() {
            Some(existing) => existing.intersection(&required).cloned().collect(),
            None => required,
        });

        if object.get("additionalProperties") != Some(&Value::Bool(false)) {
            disallow_additional_properties = false;
        }
    }

    let mut wrapped = JsonMap::from_iter([
        ("type".to_string(), Value::String("object".to_string())),
        ("properties".to_string(), Value::Object(properties)),
    ]);

    if disallow_additional_properties {
        wrapped.insert("additionalProperties".to_string(), Value::Bool(false));
    }

    if let Some(required) = required_intersection
        && !required.is_empty()
    {
        wrapped.insert(
            "required".to_string(),
            Value::Array(required.into_iter().map(Value::String).collect()),
        );
    }

    Some(Value::Object(wrapped))
}

fn merge_property_schema(properties: &mut JsonMap<String, Value>, key: &str, candidate: Value) {
    let Some(existing) = properties.get_mut(key) else {
        properties.insert(key.to_string(), candidate);
        return;
    };

    if *existing == candidate {
        return;
    }

    if let Some(enum_schema) = merge_literal_string_schemas(existing, &candidate) {
        *existing = enum_schema;
        return;
    }

    if let Some(schema) = merge_same_schema_kind_with_description(existing, &candidate) {
        *existing = schema;
        return;
    }

    *existing = merge_any_of_schemas(existing.take(), candidate);
}

fn merge_same_schema_kind_with_description(existing: &Value, candidate: &Value) -> Option<Value> {
    let existing = existing.as_object()?;
    let candidate = candidate.as_object()?;
    if existing.get("type") != candidate.get("type")
        || existing.get("enum").is_some()
        || candidate.get("enum").is_some()
    {
        return None;
    }

    let mut existing_without_description = existing.clone();
    existing_without_description.remove("description");
    let mut candidate_without_description = candidate.clone();
    candidate_without_description.remove("description");
    if existing_without_description != candidate_without_description {
        return None;
    }

    let descriptions = [existing.get("description"), candidate.get("description")]
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .fold(Vec::<String>::new(), |mut acc, value| {
            if !acc.iter().any(|item| item == value) {
                acc.push(value.to_string());
            }
            acc
        });

    let mut merged = existing_without_description;
    if !descriptions.is_empty() {
        merged.insert(
            "description".to_string(),
            Value::String(descriptions.join("\n")),
        );
    }

    Some(Value::Object(merged))
}

fn merge_literal_string_schemas(existing: &Value, candidate: &Value) -> Option<Value> {
    let mut values = literal_string_values(existing)?;
    values.extend(literal_string_values(candidate)?);
    values.sort();
    values.dedup();

    let mut schema = JsonMap::from_iter([
        ("type".to_string(), Value::String("string".to_string())),
        (
            "enum".to_string(),
            Value::Array(values.into_iter().map(Value::String).collect()),
        ),
    ]);

    if let Some(description) = existing
        .get("description")
        .and_then(Value::as_str)
        .or_else(|| candidate.get("description").and_then(Value::as_str))
    {
        schema.insert(
            "description".to_string(),
            Value::String(description.to_string()),
        );
    }

    Some(Value::Object(schema))
}

fn literal_string_values(schema: &Value) -> Option<Vec<String>> {
    let object = schema.as_object()?;
    if object.get("type")?.as_str()? != "string" {
        return None;
    }
    let values = object.get("enum")?.as_array()?;
    let mut strings = Vec::new();
    for value in values {
        strings.push(value.as_str()?.to_string());
    }
    Some(strings)
}

fn merge_any_of_schemas(existing: Value, candidate: Value) -> Value {
    let mut variants = flatten_any_of(existing);
    for schema in flatten_any_of(candidate) {
        if !variants.contains(&schema) {
            variants.push(schema);
        }
    }
    Value::Object(JsonMap::from_iter([(
        "anyOf".to_string(),
        Value::Array(variants),
    )]))
}

fn flatten_any_of(schema: Value) -> Vec<Value> {
    match schema {
        Value::Object(map) => match map.get("anyOf") {
            Some(Value::Array(variants)) => variants.clone(),
            _ => vec![Value::Object(map)],
        },
        other => vec![other],
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResponsesApiWebSearchFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allowed_domains: Option<Vec<String>>,
}

impl From<ConfigWebSearchFilters> for ResponsesApiWebSearchFilters {
    fn from(filters: ConfigWebSearchFilters) -> Self {
        Self {
            allowed_domains: filters.allowed_domains,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ResponsesApiWebSearchUserLocation {
    #[serde(rename = "type")]
    pub r#type: WebSearchUserLocationType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timezone: Option<String>,
}

impl From<ConfigWebSearchUserLocation> for ResponsesApiWebSearchUserLocation {
    fn from(user_location: ConfigWebSearchUserLocation) -> Self {
        Self {
            r#type: user_location.r#type,
            country: user_location.country,
            region: user_location.region,
            city: user_location.city,
            timezone: user_location.timezone,
        }
    }
}

#[cfg(test)]
#[path = "tool_spec_tests.rs"]
mod tests;

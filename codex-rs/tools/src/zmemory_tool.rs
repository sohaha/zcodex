use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

pub const ZMEMORY_TOOL_NAME: &str = "zmemory";

pub const ZMEMORY_MCP_TOOL_NAMES: [&str; 7] = [
    "read_memory",
    "create_memory",
    "update_memory",
    "delete_memory",
    "add_alias",
    "manage_triggers",
    "search_memory",
];

// --- Helper functions for reducing boilerplate ---

fn str_prop(name: &str, description: &str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::String {
            description: Some(description.to_string()),
        },
    )
}

fn int_prop(name: &str, description: &str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Integer {
            description: Some(description.to_string()),
        },
    )
}

fn str_array_prop(name: &str, description: &str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: Some(description.to_string()),
        },
    )
}

fn optional_str_prop(name: &str, description: &str) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::String {
            description: Some(description.to_string()),
        },
    )
}

fn mcp_tool(
    name: &str,
    description: &str,
    properties: BTreeMap<String, JsonSchema>,
    required: Vec<&str>,
) -> ToolSpec {
    ToolSpec::Function(ResponsesApiTool {
        name: name.to_string(),
        description: description.to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(required.into_iter().map(String::from).collect()),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

// --- Unified zmemory tool ---

pub fn create_zmemory_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        str_prop(
            "action",
            "Zmemory action: read | search | create | update | delete-path | add-alias | manage-triggers | stats | doctor | rebuild-search.",
        ),
        optional_str_prop("codex_home", "Optional override for CODEX_HOME."),
        optional_str_prop(
            "uri",
            "Target URI. Supports system views: system://boot|defaults|workspace|index|index/<domain>|recent|recent/<n>|glossary|alias|alias/<n>. system://defaults exposes product defaults; system://workspace exposes current workspace runtime facts.",
        ),
        optional_str_prop("parent_uri", "Parent URI for create."),
        optional_str_prop("new_uri", "New alias URI for add-alias."),
        optional_str_prop("target_uri", "Target URI for add-alias."),
        optional_str_prop("query", "Search query."),
        optional_str_prop("content", "Memory content for create."),
        optional_str_prop("title", "Optional node title for create."),
        optional_str_prop("old_string", "Update patch: old_string to replace."),
        optional_str_prop("new_string", "Update patch: new_string replacement."),
        optional_str_prop("append", "Update append: text appended to content."),
        int_prop("priority", "Priority weight for create/update."),
        optional_str_prop("disclosure", "Disclosure trigger text."),
        str_array_prop("add", "Trigger keywords to add."),
        str_array_prop("remove", "Trigger keywords to remove."),
        int_prop(
            "limit",
            "Limit results or view entries for system://defaults and system://workspace.",
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: ZMEMORY_TOOL_NAME.to_string(),
        description: "Codex embedded zmemory tool for durable memory operations.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["action".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

// --- MCP-style split tools ---

pub fn create_zmemory_mcp_tools() -> Vec<ToolSpec> {
    vec![
        mcp_tool(
            "read_memory",
            "Read a memory node or system view.",
            BTreeMap::from([str_prop(
                "uri",
                "Memory URI, e.g. core://agent or system://boot.",
            )]),
            vec!["uri"],
        ),
        mcp_tool(
            "search_memory",
            "Search memory content and paths using full-text search.",
            BTreeMap::from([
                str_prop("query", "Search query."),
                optional_str_prop("domain", "Optional domain scope (e.g. core, project)."),
                int_prop("limit", "Maximum results to return."),
            ]),
            vec!["query"],
        ),
        mcp_tool(
            "create_memory",
            "Create a new memory node under a parent URI.",
            BTreeMap::from([
                optional_str_prop("parent_uri", "Parent URI to create under (e.g. core://)."),
                str_prop("content", "Memory content."),
                int_prop("priority", "Priority weight; lower is more important."),
                optional_str_prop("title", "Optional leaf title for the new path."),
                optional_str_prop("disclosure", "Optional recall trigger text."),
            ]),
            vec!["parent_uri", "content", "priority"],
        ),
        mcp_tool(
            "update_memory",
            "Update a memory using patch or append mode.",
            BTreeMap::from([
                str_prop("uri", "Target memory URI."),
                optional_str_prop("old_string", "Patch mode: original text to replace."),
                optional_str_prop("new_string", "Patch mode: replacement text."),
                optional_str_prop("append", "Append mode: text appended to content."),
                int_prop("priority", "Optional priority update for this path."),
                optional_str_prop("disclosure", "Optional disclosure update for this path."),
            ]),
            vec!["uri"],
        ),
        mcp_tool(
            "delete_memory",
            "Delete a memory path (does not erase underlying content).",
            BTreeMap::from([str_prop("uri", "URI path to remove.")]),
            vec!["uri"],
        ),
        mcp_tool(
            "add_alias",
            "Create an alias path to an existing memory.",
            BTreeMap::from([
                str_prop("new_uri", "New alias URI."),
                str_prop("target_uri", "Target URI to alias."),
                int_prop("priority", "Optional priority for the alias path."),
                optional_str_prop("disclosure", "Optional disclosure for the alias path."),
            ]),
            vec!["new_uri", "target_uri"],
        ),
        mcp_tool(
            "manage_triggers",
            "Manage trigger keywords for a memory node.",
            BTreeMap::from([
                str_prop("uri", "Target memory URI."),
                str_array_prop("add", "Keywords to add."),
                str_array_prop("remove", "Keywords to remove."),
            ]),
            vec!["uri"],
        ),
    ]
}

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

/// Helper to build a string property with an optional description.
fn str_prop(name: &str, desc: Option<&str>) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::String {
            description: desc.map(ToString::to_string),
        },
    )
}

/// Helper to build an integer property with an optional description.
fn int_prop(name: &str, desc: Option<&str>) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Integer {
            description: desc.map(ToString::to_string),
        },
    )
}

/// Helper to build a string-array property with an optional description.
fn str_array_prop(name: &str, desc: Option<&str>) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::String { description: None }),
            description: desc.map(ToString::to_string),
        },
    )
}

/// Build a ToolSpec from name, description, properties, and required fields.
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
            required: Some(required.into_iter().map(ToString::to_string).collect()),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

pub fn create_zmemory_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        str_prop(
            "action",
            Some(
                "Zmemory action: read | search | create | update | delete-path | add-alias | manage-triggers | stats | doctor | rebuild-search.",
            ),
        ),
        str_prop("codex_home", Some("Optional override for CODEX_HOME.")),
        str_prop(
            "uri",
            Some(
                "Target URI. Supports system views: system://boot|defaults|workspace|index|index/<domain>|recent|recent/<n>|glossary|alias|alias/<n>. system://defaults exposes product defaults; system://workspace exposes current workspace runtime facts.",
            ),
        ),
        str_prop("parent_uri", Some("Parent URI for create.")),
        str_prop("new_uri", Some("New alias URI for add-alias.")),
        str_prop("target_uri", Some("Target URI for add-alias.")),
        str_prop("query", Some("Search query.")),
        str_prop("content", Some("Memory content for create.")),
        str_prop("title", Some("Optional node title for create.")),
        str_prop("old_string", Some("Update patch: old_string to replace.")),
        str_prop("new_string", Some("Update patch: new_string replacement.")),
        str_prop("append", Some("Update append: text appended to content.")),
        int_prop("priority", Some("Priority weight for create/update.")),
        str_prop("disclosure", Some("Disclosure trigger text.")),
        str_array_prop("add", Some("Trigger keywords to add.")),
        str_array_prop("remove", Some("Trigger keywords to remove.")),
        int_prop(
            "limit",
            Some("Limit results or view entries for system://defaults and system://workspace."),
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

pub fn create_zmemory_mcp_tools() -> Vec<ToolSpec> {
    vec![
        mcp_tool(
            "read_memory",
            "Read a memory node or system view.",
            BTreeMap::from([str_prop(
                "uri",
                Some("Memory URI, e.g. core://agent or system://boot."),
            )]),
            vec!["uri"],
        ),
        mcp_tool(
            "search_memory",
            "Search memory content and paths using full-text search.",
            BTreeMap::from([
                str_prop("query", Some("Search query.")),
                str_prop(
                    "domain",
                    Some("Optional domain scope (e.g. core, project)."),
                ),
                int_prop("limit", Some("Maximum results to return.")),
            ]),
            vec!["query"],
        ),
        mcp_tool(
            "create_memory",
            "Create a new memory node under a parent URI.",
            BTreeMap::from([
                str_prop(
                    "parent_uri",
                    Some("Parent URI to create under (e.g. core://)."),
                ),
                str_prop("content", Some("Memory content.")),
                int_prop(
                    "priority",
                    Some("Priority weight; lower is more important."),
                ),
                str_prop("title", Some("Optional leaf title for the new path.")),
                str_prop("disclosure", Some("Optional recall trigger text.")),
            ]),
            vec!["parent_uri", "content", "priority"],
        ),
        mcp_tool(
            "update_memory",
            "Update a memory using patch or append mode.",
            BTreeMap::from([
                str_prop("uri", Some("Target memory URI.")),
                str_prop("old_string", Some("Patch mode: original text to replace.")),
                str_prop("new_string", Some("Patch mode: replacement text.")),
                str_prop("append", Some("Append mode: text appended to content.")),
                int_prop("priority", Some("Optional priority update for this path.")),
                str_prop(
                    "disclosure",
                    Some("Optional disclosure update for this path."),
                ),
            ]),
            vec!["uri"],
        ),
        mcp_tool(
            "delete_memory",
            "Delete a memory path (does not erase underlying content).",
            BTreeMap::from([str_prop("uri", Some("URI path to remove."))]),
            vec!["uri"],
        ),
        mcp_tool(
            "add_alias",
            "Create an alias path to an existing memory.",
            BTreeMap::from([
                str_prop("new_uri", Some("New alias URI.")),
                str_prop("target_uri", Some("Target URI to alias.")),
                int_prop("priority", Some("Optional priority for the alias path.")),
                str_prop(
                    "disclosure",
                    Some("Optional disclosure for the alias path."),
                ),
            ]),
            vec!["new_uri", "target_uri"],
        ),
        mcp_tool(
            "manage_triggers",
            "Manage trigger keywords for a memory node.",
            BTreeMap::from([
                str_prop("uri", Some("Target memory URI.")),
                str_array_prop("add", Some("Keywords to add.")),
                str_array_prop("remove", Some("Keywords to remove.")),
            ]),
            vec!["uri"],
        ),
    ]
}

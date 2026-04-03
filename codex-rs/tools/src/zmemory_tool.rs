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

pub fn create_zmemory_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "action".to_string(),
            JsonSchema::String {
                description: Some(
                    "Zmemory action: read | search | create | update | delete-path | add-alias | manage-triggers | stats | doctor | rebuild-search.".to_string(),
                ),
            },
        ),
        (
            "codex_home".to_string(),
            JsonSchema::String {
                description: Some("Optional override for CODEX_HOME.".to_string()),
            },
        ),
        (
            "uri".to_string(),
            JsonSchema::String {
                description: Some(
                    "Target URI. Supports system views: system://boot|defaults|workspace|index|index/<domain>|recent|recent/<n>|glossary|alias|alias/<n>. system://defaults exposes product defaults; system://workspace exposes current workspace runtime facts.".to_string(),
                ),
            },
        ),
        (
            "parent_uri".to_string(),
            JsonSchema::String {
                description: Some("Parent URI for create.".to_string()),
            },
        ),
        (
            "new_uri".to_string(),
            JsonSchema::String {
                description: Some("New alias URI for add-alias.".to_string()),
            },
        ),
        (
            "target_uri".to_string(),
            JsonSchema::String {
                description: Some("Target URI for add-alias.".to_string()),
            },
        ),
        (
            "query".to_string(),
            JsonSchema::String {
                description: Some("Search query.".to_string()),
            },
        ),
        (
            "content".to_string(),
            JsonSchema::String {
                description: Some("Memory content for create.".to_string()),
            },
        ),
        (
            "title".to_string(),
            JsonSchema::String {
                description: Some("Optional node title for create.".to_string()),
            },
        ),
        (
            "old_string".to_string(),
            JsonSchema::String {
                description: Some("Update patch: old_string to replace.".to_string()),
            },
        ),
        (
            "new_string".to_string(),
            JsonSchema::String {
                description: Some("Update patch: new_string replacement.".to_string()),
            },
        ),
        (
            "append".to_string(),
            JsonSchema::String {
                description: Some("Update append: text appended to content.".to_string()),
            },
        ),
        (
            "priority".to_string(),
            JsonSchema::Number {
                description: Some("Priority weight for create/update.".to_string()),
            },
        ),
        (
            "disclosure".to_string(),
            JsonSchema::String {
                description: Some("Disclosure trigger text.".to_string()),
            },
        ),
        (
            "add".to_string(),
            JsonSchema::Array {
                items: Box::new(JsonSchema::String { description: None }),
                description: Some("Trigger keywords to add.".to_string()),
            },
        ),
        (
            "remove".to_string(),
            JsonSchema::Array {
                items: Box::new(JsonSchema::String { description: None }),
                description: Some("Trigger keywords to remove.".to_string()),
            },
        ),
        (
            "limit".to_string(),
            JsonSchema::Number {
                description: Some(
                    "Limit results or view entries for system://defaults and system://workspace.".to_string(),
                ),
            },
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
        create_read_memory_tool(),
        create_create_memory_tool(),
        create_update_memory_tool(),
        create_delete_memory_tool(),
        create_add_alias_tool(),
        create_manage_triggers_tool(),
        create_search_memory_tool(),
    ]
}

fn create_read_memory_tool() -> ToolSpec {
    let properties = BTreeMap::from([(
        "uri".to_string(),
        JsonSchema::String {
            description: Some("Memory URI, e.g. core://agent or system://boot.".to_string()),
        },
    )]);
    ToolSpec::Function(ResponsesApiTool {
        name: "read_memory".to_string(),
        description: "Read a memory node or system view.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["uri".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

fn create_search_memory_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "query".to_string(),
            JsonSchema::String {
                description: Some("Search query.".to_string()),
            },
        ),
        (
            "domain".to_string(),
            JsonSchema::String {
                description: Some("Optional domain scope (e.g. core, project).".to_string()),
            },
        ),
        (
            "limit".to_string(),
            JsonSchema::Number {
                description: Some("Maximum results to return.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "search_memory".to_string(),
        description: "Search memory content and paths using full-text search.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["query".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

fn create_create_memory_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "parent_uri".to_string(),
            JsonSchema::String {
                description: Some("Parent URI to create under (e.g. core://).".to_string()),
            },
        ),
        (
            "content".to_string(),
            JsonSchema::String {
                description: Some("Memory content.".to_string()),
            },
        ),
        (
            "priority".to_string(),
            JsonSchema::Number {
                description: Some("Priority weight; lower is more important.".to_string()),
            },
        ),
        (
            "title".to_string(),
            JsonSchema::String {
                description: Some("Optional leaf title for the new path.".to_string()),
            },
        ),
        (
            "disclosure".to_string(),
            JsonSchema::String {
                description: Some("Optional recall trigger text.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "create_memory".to_string(),
        description: "Create a new memory node under a parent URI.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec![
                "parent_uri".to_string(),
                "content".to_string(),
                "priority".to_string(),
            ]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

fn create_update_memory_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "uri".to_string(),
            JsonSchema::String {
                description: Some("Target memory URI.".to_string()),
            },
        ),
        (
            "old_string".to_string(),
            JsonSchema::String {
                description: Some("Patch mode: original text to replace.".to_string()),
            },
        ),
        (
            "new_string".to_string(),
            JsonSchema::String {
                description: Some("Patch mode: replacement text.".to_string()),
            },
        ),
        (
            "append".to_string(),
            JsonSchema::String {
                description: Some("Append mode: text appended to content.".to_string()),
            },
        ),
        (
            "priority".to_string(),
            JsonSchema::Number {
                description: Some("Optional priority update for this path.".to_string()),
            },
        ),
        (
            "disclosure".to_string(),
            JsonSchema::String {
                description: Some("Optional disclosure update for this path.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "update_memory".to_string(),
        description: "Update a memory using patch or append mode.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["uri".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

fn create_delete_memory_tool() -> ToolSpec {
    let properties = BTreeMap::from([(
        "uri".to_string(),
        JsonSchema::String {
            description: Some("URI path to remove.".to_string()),
        },
    )]);

    ToolSpec::Function(ResponsesApiTool {
        name: "delete_memory".to_string(),
        description: "Delete a memory path (does not erase underlying content).".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["uri".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

fn create_add_alias_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "new_uri".to_string(),
            JsonSchema::String {
                description: Some("New alias URI.".to_string()),
            },
        ),
        (
            "target_uri".to_string(),
            JsonSchema::String {
                description: Some("Target URI to alias.".to_string()),
            },
        ),
        (
            "priority".to_string(),
            JsonSchema::Number {
                description: Some("Optional priority for the alias path.".to_string()),
            },
        ),
        (
            "disclosure".to_string(),
            JsonSchema::String {
                description: Some("Optional disclosure for the alias path.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "add_alias".to_string(),
        description: "Create an alias path to an existing memory.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["new_uri".to_string(), "target_uri".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

fn create_manage_triggers_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "uri".to_string(),
            JsonSchema::String {
                description: Some("Target memory URI.".to_string()),
            },
        ),
        (
            "add".to_string(),
            JsonSchema::Array {
                items: Box::new(JsonSchema::String { description: None }),
                description: Some("Keywords to add.".to_string()),
            },
        ),
        (
            "remove".to_string(),
            JsonSchema::Array {
                items: Box::new(JsonSchema::String { description: None }),
                description: Some("Keywords to remove.".to_string()),
            },
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: "manage_triggers".to_string(),
        description: "Manage trigger keywords for a memory node.".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::Object {
            properties,
            required: Some(vec!["uri".to_string()]),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

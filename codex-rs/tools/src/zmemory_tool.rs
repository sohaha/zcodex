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
                "Zmemory 操作：read | search | create | update | delete-path | add-alias | manage-triggers | stats | doctor | rebuild-search。",
            ),
        ),
        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
        str_prop(
            "uri",
            Some(
                "目标 URI。支持系统视图：system://boot|defaults|workspace|index|index/<domain>|recent|recent/<n>|glossary|alias|alias/<n>。system://defaults 暴露产品默认值；system://workspace 暴露当前工作区运行时事实。",
            ),
        ),
        str_prop("parent_uri", Some("create 操作使用的父级 URI。")),
        str_prop("new_uri", Some("add-alias 操作的新别名 URI。")),
        str_prop("target_uri", Some("add-alias 操作的目标 URI。")),
        str_prop("query", Some("搜索查询。")),
        str_prop("content", Some("create 操作的记忆内容。")),
        str_prop("title", Some("create 操作可选的节点标题。")),
        str_prop("old_string", Some("update 补丁模式中待替换的 old_string。")),
        str_prop(
            "new_string",
            Some("update 补丁模式中的 new_string 替换内容。"),
        ),
        str_prop("append", Some("update 追加模式中附加到内容末尾的文本。")),
        int_prop("priority", Some("create/update 使用的优先级权重。")),
        str_prop("disclosure", Some("披露触发文本。")),
        str_array_prop("add", Some("要新增的触发关键词。")),
        str_array_prop("remove", Some("要移除的触发关键词。")),
        int_prop(
            "limit",
            Some("system://defaults 与 system://workspace 的结果或视图条目上限。"),
        ),
    ]);

    mcp_tool(
        ZMEMORY_TOOL_NAME,
        "Codex 内置的 zmemory 持久记忆工具。",
        properties,
        vec!["action"],
    )
}

pub fn create_zmemory_mcp_tools() -> Vec<ToolSpec> {
    vec![
        mcp_tool(
            "read_memory",
            "读取某条记忆或系统视图。",
            BTreeMap::from([str_prop(
                "uri",
                Some("记忆 URI，例如 core://agent 或 system://boot。"),
            )]),
            vec!["uri"],
        ),
        mcp_tool(
            "search_memory",
            "使用全文检索搜索记忆内容和路径。",
            BTreeMap::from([
                str_prop("query", Some("搜索查询。")),
                str_prop("domain", Some("可选的域范围（例如 core、project）。")),
                int_prop("limit", Some("返回结果上限。")),
            ]),
            vec!["query"],
        ),
        mcp_tool(
            "create_memory",
            "在父级 URI 下创建新的记忆节点。",
            BTreeMap::from([
                str_prop(
                    "parent_uri",
                    Some("要创建到其下的父级 URI（例如 core://）。"),
                ),
                str_prop("content", Some("记忆内容。")),
                int_prop("priority", Some("优先级权重；数值越小越重要。")),
                str_prop("title", Some("新路径可选的叶子标题。")),
                str_prop("disclosure", Some("可选的 recall 触发文本。")),
            ]),
            vec!["parent_uri", "content", "priority"],
        ),
        mcp_tool(
            "update_memory",
            "使用补丁或追加模式更新记忆。",
            BTreeMap::from([
                str_prop("uri", Some("目标记忆 URI。")),
                str_prop("old_string", Some("补丁模式中待替换的原始文本。")),
                str_prop("new_string", Some("补丁模式中的替换文本。")),
                str_prop("append", Some("追加模式中附加到内容末尾的文本。")),
                int_prop("priority", Some("该路径可选的优先级更新。")),
                str_prop("disclosure", Some("该路径可选的 disclosure 更新。")),
            ]),
            vec!["uri"],
        ),
        mcp_tool(
            "delete_memory",
            "删除记忆路径（不会擦除底层内容）。",
            BTreeMap::from([str_prop("uri", Some("要移除的 URI 路径。"))]),
            vec!["uri"],
        ),
        mcp_tool(
            "add_alias",
            "为现有记忆创建别名路径。",
            BTreeMap::from([
                str_prop("new_uri", Some("新的别名 URI。")),
                str_prop("target_uri", Some("要指向的目标 URI。")),
                int_prop("priority", Some("别名路径可选的优先级。")),
                str_prop("disclosure", Some("别名路径可选的 disclosure。")),
            ]),
            vec!["new_uri", "target_uri"],
        ),
        mcp_tool(
            "manage_triggers",
            "管理某条记忆的触发关键词。",
            BTreeMap::from([
                str_prop("uri", Some("目标记忆 URI。")),
                str_array_prop("add", Some("要新增的关键词。")),
                str_array_prop("remove", Some("要移除的关键词。")),
            ]),
            vec!["uri"],
        ),
    ]
}

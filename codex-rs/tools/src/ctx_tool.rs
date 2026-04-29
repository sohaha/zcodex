use crate::JsonSchema;
use crate::ResponsesApiTool;
use crate::ToolSpec;
use std::collections::BTreeMap;

pub const CTX_TOOL_NAMES: [&str; 5] = [
    "ctx_search",
    "ctx_stats",
    "ctx_doctor",
    "ctx_purge",
    "ctx_execute",
];

fn str_prop(name: &str, desc: Option<&str>) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::String {
            description: desc.map(ToString::to_string),
        },
    )
}

fn int_prop(name: &str, desc: Option<&str>) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Integer {
            description: desc.map(ToString::to_string),
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
            required: Some(required.into_iter().map(ToString::to_string).collect()),
            additional_properties: Some(false.into()),
        },
        output_schema: None,
    })
}

pub fn create_ctx_tools() -> Vec<ToolSpec> {
    vec![
        ctx_search_tool(),
        ctx_stats_tool(),
        ctx_doctor_tool(),
        ctx_purge_tool(),
        ctx_execute_tool(),
    ]
}

fn ctx_search_tool() -> ToolSpec {
    mcp_tool(
        "ctx_search",
        "搜索当前 session 的上下文事件记录。返回匹配的事件摘要，用于回顾会话历史中的工具调用、文件编辑和错误。",
        BTreeMap::from([
            str_prop("query", Some("搜索关键词。")),
            str_prop(
                "session_id",
                Some("目标 session ID。不填则搜索当前 session。"),
            ),
            int_prop("limit", Some("返回结果上限。")),
        ]),
        vec!["query"],
    )
}

fn ctx_stats_tool() -> ToolSpec {
    mcp_tool(
        "ctx_stats",
        "查看当前 session 的上下文统计信息：已记录事件数、分类分布、存储占用等。",
        BTreeMap::from([str_prop(
            "session_id",
            Some("目标 session ID。不填则统计当前 session。"),
        )]),
        vec![],
    )
}

fn ctx_doctor_tool() -> ToolSpec {
    mcp_tool(
        "ctx_doctor",
        "诊断 zcontext 子系统健康状态：存储可用性、索引完整性、配置一致性。",
        BTreeMap::new(),
        vec![],
    )
}

fn ctx_purge_tool() -> ToolSpec {
    mcp_tool(
        "ctx_purge",
        "清理指定 session 或过期 session 的上下文记录。释放存储空间。",
        BTreeMap::from([str_prop(
            "session_id",
            Some("要清理的 session ID。不填则清理所有过期 session。"),
        )]),
        vec![],
    )
}

fn ctx_execute_tool() -> ToolSpec {
    mcp_tool(
        "ctx_execute",
        "直接执行 zmemory 操作，用于高级上下文管理场景。action 参数与 zmemory tool 对齐。",
        BTreeMap::from([
            str_prop(
                "action",
                Some("zmemory action：read, search, create, update, delete-path, stats, doctor。"),
            ),
            str_prop("uri", Some("目标 URI。")),
            str_prop("query", Some("搜索查询（search action 时使用）。")),
            str_prop("content", Some("内容（create/update action 时使用）。")),
            int_prop("limit", Some("结果条目上限。")),
        ]),
        vec!["action"],
    )
}

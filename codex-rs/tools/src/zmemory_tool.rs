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

fn literal_str_prop(name: &str, value: &str, desc: Option<&str>) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::LiteralString {
            value: value.to_string(),
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

fn array_of_object_prop(
    name: &str,
    desc: Option<&str>,
    properties: BTreeMap<String, JsonSchema>,
    required: Vec<&str>,
) -> (String, JsonSchema) {
    (
        name.to_string(),
        JsonSchema::Array {
            items: Box::new(JsonSchema::Object {
                properties,
                required: Some(required.into_iter().map(ToString::to_string).collect()),
                additional_properties: Some(false.into()),
            }),
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
    ToolSpec::Function(ResponsesApiTool {
        name: ZMEMORY_TOOL_NAME.to_string(),
        description: "Codex 内置的 zmemory 持久记忆工具。".to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::OneOf {
            variants: vec![
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "read", Some("读取记忆或系统视图。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop(
                            "uri",
                            Some(
                                "目标 URI。支持系统视图：system://boot|defaults|workspace|index|index/<domain>|system://paths|system://paths/<domain>|recent|recent/<n>|glossary|alias|alias/<n>。",
                            ),
                        ),
                        int_prop("limit", Some("system 视图的结果条目上限。")),
                    ]),
                    required: Some(vec!["action".to_string(), "uri".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "history", Some("查看某条记忆的版本历史。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("uri", Some("目标 URI。")),
                    ]),
                    required: Some(vec!["action".to_string(), "uri".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "search", Some("全文检索。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("query", Some("搜索查询。")),
                        str_prop("uri", Some("可选的 URI scope。")),
                        int_prop("limit", Some("返回结果上限。")),
                    ]),
                    required: Some(vec!["action".to_string(), "query".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop(
                            "action",
                            "export",
                            Some("导出真实记忆数据；需且仅需提供 uri。"),
                        ),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("uri", Some("要导出的目标 URI。")),
                    ]),
                    required: Some(vec!["action".to_string(), "uri".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop(
                            "action",
                            "export",
                            Some("导出真实记忆数据；需且仅需提供 domain。"),
                        ),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("domain", Some("要导出的裸域名，例如 core、project。")),
                    ]),
                    required: Some(vec!["action".to_string(), "domain".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "import", Some("导入真实记忆数据。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        array_of_object_prop(
                            "items",
                            Some("待导入的记忆数组。"),
                            BTreeMap::from([
                                str_prop("uri", Some("目标 URI。")),
                                str_prop("content", Some("记忆内容。")),
                                int_prop("priority", Some("优先级权重。")),
                                str_prop("disclosure", Some("可选 disclosure。")),
                                str_array_prop("keywords", Some("可选触发关键词数组。")),
                                array_of_object_prop(
                                    "aliases",
                                    Some("可选 alias 数组。"),
                                    BTreeMap::from([
                                        str_prop("uri", Some("alias URI。")),
                                        int_prop("priority", Some("可选 alias 优先级。")),
                                        str_prop("disclosure", Some("可选 alias disclosure。")),
                                    ]),
                                    vec!["uri"],
                                ),
                            ]),
                            vec!["uri", "content"],
                        ),
                    ]),
                    required: Some(vec!["action".to_string(), "items".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "create", Some("创建记忆节点。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("uri", Some("目标 URI。")),
                        str_prop("parent_uri", Some("父级 URI。")),
                        str_prop("content", Some("记忆内容。")),
                        str_prop("title", Some("可选节点标题。")),
                        int_prop("priority", Some("优先级权重。")),
                        str_prop("disclosure", Some("披露触发文本。")),
                    ]),
                    required: Some(vec!["action".to_string(), "content".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "update", Some("更新记忆节点。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("uri", Some("目标 URI。")),
                        str_prop("content", Some("完整替换内容。")),
                        str_prop("old_string", Some("补丁模式待替换文本。")),
                        str_prop("new_string", Some("补丁模式替换文本。")),
                        str_prop("append", Some("追加文本。")),
                        int_prop("priority", Some("可选优先级更新。")),
                        str_prop("disclosure", Some("可选 disclosure 更新。")),
                    ]),
                    required: Some(vec!["action".to_string(), "uri".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "delete-path", Some("删除记忆路径。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("uri", Some("要移除的 URI 路径。")),
                    ]),
                    required: Some(vec!["action".to_string(), "uri".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "add-alias", Some("创建别名路径。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("new_uri", Some("新的别名 URI。")),
                        str_prop("target_uri", Some("目标 URI。")),
                        int_prop("priority", Some("可选优先级。")),
                        str_prop("disclosure", Some("可选 disclosure。")),
                    ]),
                    required: Some(vec![
                        "action".to_string(),
                        "new_uri".to_string(),
                        "target_uri".to_string(),
                    ]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "manage-triggers", Some("管理触发词。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        str_prop("uri", Some("目标 URI。")),
                        str_array_prop("add", Some("要新增的触发关键词。")),
                        str_array_prop("remove", Some("要移除的触发关键词。")),
                    ]),
                    required: Some(vec!["action".to_string(), "uri".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "batch-create", Some("批量创建记忆节点。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        array_of_object_prop(
                            "items",
                            Some("批量创建的数据数组。"),
                            BTreeMap::from([
                                str_prop("uri", Some("目标 URI。")),
                                str_prop("parent_uri", Some("父级 URI。")),
                                str_prop("content", Some("记忆内容。")),
                                str_prop("title", Some("可选节点标题。")),
                                int_prop("priority", Some("优先级权重。")),
                                str_prop("disclosure", Some("披露触发文本。")),
                            ]),
                            vec!["content"],
                        ),
                    ]),
                    required: Some(vec!["action".to_string(), "items".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "batch-update", Some("批量更新记忆节点。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        array_of_object_prop(
                            "items",
                            Some("批量更新的数据数组。"),
                            BTreeMap::from([
                                str_prop("uri", Some("目标 URI。")),
                                str_prop("content", Some("完整替换内容。")),
                                str_prop("old_string", Some("补丁模式待替换文本。")),
                                str_prop("new_string", Some("补丁模式替换文本。")),
                                str_prop("append", Some("追加文本。")),
                                int_prop("priority", Some("可选优先级更新。")),
                                str_prop("disclosure", Some("可选 disclosure 更新。")),
                            ]),
                            vec!["uri"],
                        ),
                    ]),
                    required: Some(vec!["action".to_string(), "items".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "stats", Some("查看统计。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                    ]),
                    required: Some(vec!["action".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "audit", Some("查看最近审计日志。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                        int_prop("limit", Some("返回最近审计记录条数。")),
                        str_prop(
                            "audit_action",
                            Some("可选：按审计动作过滤，例如 create、update。"),
                        ),
                        str_prop("uri", Some("可选：按目标 URI 精确过滤。")),
                    ]),
                    required: Some(vec!["action".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "doctor", Some("健康检查。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                    ]),
                    required: Some(vec!["action".to_string()]),
                    additional_properties: Some(false.into()),
                },
                JsonSchema::Object {
                    properties: BTreeMap::from([
                        literal_str_prop("action", "rebuild-search", Some("重建搜索索引。")),
                        str_prop("codex_home", Some("可选的 CODEX_HOME 覆盖路径。")),
                    ]),
                    required: Some(vec!["action".to_string()]),
                    additional_properties: Some(false.into()),
                },
            ],
        },
        output_schema: None,
    })
}

pub fn create_zmemory_mcp_tools() -> Vec<ToolSpec> {
    vec![
        mcp_tool(
            "read_memory",
            "读取某条记忆或系统视图。",
            BTreeMap::from([str_prop(
                "uri",
                Some("记忆 URI，例如 core://agent、system://boot 或 system://paths。"),
            )]),
            vec!["uri"],
        ),
        mcp_tool(
            "search_memory",
            "使用全文检索搜索记忆内容和路径。",
            BTreeMap::from([
                str_prop("query", Some("搜索查询。")),
                str_prop(
                    "uri",
                    Some("可选的 URI scope，例如 core://、core://team 或 project://。"),
                ),
                str_prop(
                    "domain",
                    Some(
                        "兼容旧字段：可选的域范围（例如 core、project），会自动转换为 <domain>:// scope。",
                    ),
                ),
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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn function_tool(spec: ToolSpec) -> ResponsesApiTool {
        match spec {
            ToolSpec::Function(tool) => tool,
            other => panic!("expected function tool, got {other:?}"),
        }
    }

    #[test]
    fn zmemory_tool_exposes_one_of_action_variants() {
        let tool = function_tool(create_zmemory_tool());
        let JsonSchema::OneOf { variants } = tool.parameters else {
            panic!("zmemory tool should expose oneOf parameters");
        };
        assert_eq!(variants.len(), 17);
    }

    #[test]
    fn zmemory_tool_read_variant_uri_description_mentions_paths_views() {
        let tool = function_tool(create_zmemory_tool());
        let JsonSchema::OneOf { variants } = tool.parameters else {
            panic!("zmemory tool should expose oneOf parameters");
        };
        let JsonSchema::Object { properties, .. } = &variants[0] else {
            panic!("read variant should be object parameters");
        };

        let Some(JsonSchema::String {
            description: Some(description),
        }) = properties.get("uri")
        else {
            panic!("uri property should expose a description");
        };

        assert!(description.contains("system://paths"));
        assert!(description.contains("paths"));
        assert!(description.contains("workspace"));
    }

    #[test]
    fn zmemory_tool_read_variant_action_is_literal() {
        let tool = function_tool(create_zmemory_tool());
        let JsonSchema::OneOf { variants } = tool.parameters else {
            panic!("zmemory tool should expose oneOf parameters");
        };
        let JsonSchema::Object { properties, .. } = &variants[0] else {
            panic!("read variant should be object parameters");
        };

        let Some(JsonSchema::LiteralString { value, .. }) = properties.get("action") else {
            panic!("action property should be a literal string");
        };
        assert_eq!(value, "read");
    }

    #[test]
    fn zmemory_tool_export_variants_require_uri_or_domain() {
        let tool = function_tool(create_zmemory_tool());
        let JsonSchema::OneOf { variants } = tool.parameters else {
            panic!("zmemory tool should expose oneOf parameters");
        };

        let export_required_sets = variants
            .iter()
            .filter_map(|variant| {
                let JsonSchema::Object {
                    properties,
                    required,
                    ..
                } = variant
                else {
                    return None;
                };
                let Some(JsonSchema::LiteralString { value, .. }) = properties.get("action") else {
                    return None;
                };
                (value == "export").then(|| required.clone().unwrap_or_default())
            })
            .collect::<Vec<_>>();

        assert_eq!(
            export_required_sets,
            vec![
                vec!["action".to_string(), "uri".to_string()],
                vec!["action".to_string(), "domain".to_string()],
            ]
        );
    }

    #[test]
    fn zmemory_tool_import_variant_requires_items_and_mentions_aliases() {
        let tool = function_tool(create_zmemory_tool());
        let JsonSchema::OneOf { variants } = tool.parameters else {
            panic!("zmemory tool should expose oneOf parameters");
        };
        let import_variant = variants
            .iter()
            .find_map(|variant| {
                let JsonSchema::Object {
                    properties,
                    required,
                    ..
                } = variant
                else {
                    return None;
                };
                let Some(JsonSchema::LiteralString { value, .. }) = properties.get("action") else {
                    return None;
                };
                (value == "import").then_some((properties, required))
            })
            .expect("import variant should exist");

        assert_eq!(
            import_variant.1.clone(),
            Some(vec!["action".to_string(), "items".to_string()])
        );
        let Some(JsonSchema::Array { items, .. }) = import_variant.0.get("items") else {
            panic!("import items should be an array");
        };
        let JsonSchema::Object { properties, .. } = items.as_ref() else {
            panic!("import items should contain object rows");
        };
        assert!(properties.contains_key("keywords"));
        assert!(properties.contains_key("aliases"));
    }

    #[test]
    fn search_memory_schema_prefers_uri_scope_and_keeps_domain_compat() {
        let tool = function_tool(
            create_zmemory_mcp_tools()
                .into_iter()
                .find(|spec| spec.name() == "search_memory")
                .expect("search_memory tool should exist"),
        );
        let JsonSchema::Object {
            properties,
            required,
            ..
        } = tool.parameters
        else {
            panic!("search_memory should expose object parameters");
        };

        assert_eq!(required, Some(vec!["query".to_string()]));
        let Some(JsonSchema::String {
            description: Some(uri_description),
        }) = properties.get("uri")
        else {
            panic!("search_memory uri description should exist");
        };
        let Some(JsonSchema::String {
            description: Some(domain_description),
        }) = properties.get("domain")
        else {
            panic!("search_memory domain description should exist");
        };
        assert!(uri_description.contains("URI scope"));
        assert!(domain_description.contains("兼容旧字段"));
    }

    #[test]
    fn delete_memory_schema_mentions_path_only_contract() {
        let tool = function_tool(
            create_zmemory_mcp_tools()
                .into_iter()
                .find(|spec| spec.name() == "delete_memory")
                .expect("delete_memory tool should exist"),
        );
        assert!(tool.description.contains("不会擦除底层内容"));
    }

    #[test]
    fn create_zmemory_mcp_tools_keep_required_fields_stable() {
        let required_fields = create_zmemory_mcp_tools()
            .into_iter()
            .map(function_tool)
            .map(|tool| {
                let JsonSchema::Object { required, .. } = tool.parameters else {
                    panic!("tool should expose object parameters");
                };
                (tool.name, required.unwrap_or_default())
            })
            .collect::<BTreeMap<_, _>>();

        assert_eq!(
            required_fields,
            BTreeMap::from([
                (
                    "add_alias".to_string(),
                    vec!["new_uri".to_string(), "target_uri".to_string()]
                ),
                (
                    "create_memory".to_string(),
                    vec![
                        "parent_uri".to_string(),
                        "content".to_string(),
                        "priority".to_string(),
                    ]
                ),
                ("delete_memory".to_string(), vec!["uri".to_string()]),
                ("manage_triggers".to_string(), vec!["uri".to_string()]),
                ("read_memory".to_string(), vec!["uri".to_string()]),
                ("search_memory".to_string(), vec!["query".to_string()]),
                ("update_memory".to_string(), vec!["uri".to_string()]),
            ])
        );
    }
}

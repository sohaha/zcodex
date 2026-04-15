use codex_utils_home_dir::find_codex_home;
use codex_zmemory::tool_api::ZmemoryToolCallParam;
use codex_zmemory::tool_api::ZmemoryToolResult;
use codex_zmemory::tool_api::run_zmemory_tool as exec_zmemory;
use codex_zmemory::tool_api::zmemory_tool_output_schema;
use rmcp::model::CallToolResult;
use rmcp::model::Content;
use rmcp::model::JsonObject;
use rmcp::model::Tool;
use schemars::r#gen::SchemaSettings;
use std::sync::Arc;

pub(crate) fn create_tool_for_zmemory_tool_call_param() -> Tool {
    let schema = SchemaSettings::draft2019_09()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.option_add_null_type = false;
        })
        .into_generator()
        .into_root_schema_for::<ZmemoryToolCallParam>();
    let input_schema = create_tool_input_schema(schema);

    Tool {
        name: "zmemory".into(),
        title: Some("Codex ZMemory".to_string()),
        description: Some(
            "Persistent long-term workspace memory graph. Use for saving and maintaining durable workspace knowledge across sessions.".into(),
        ),
        input_schema,
        output_schema: Some(match zmemory_tool_output_schema() {
            serde_json::Value::Object(map) => Arc::new(map),
            _ => unreachable!("json literal must be an object"),
        }),
        annotations: None,
        execution: None,
        icons: None,
        meta: None,
    }
}

pub(crate) async fn run_zmemory_tool(arguments: Option<JsonObject>) -> CallToolResult {
    let args = match arguments.map(serde_json::Value::Object) {
        Some(json_val) => match serde_json::from_value::<ZmemoryToolCallParam>(json_val) {
            Ok(args) => args,
            Err(err) => {
                return error_result(format!("Failed to parse zmemory tool arguments: {err}"));
            }
        },
        None => return error_result("Missing arguments for zmemory tool-call.".to_string()),
    };

    let codex_home = match find_codex_home() {
        Ok(p) => p.to_path_buf(),
        Err(err) => return error_result(format!("Cannot determine CODEX_HOME: {err}")),
    };

    // zmemory operations are synchronous; run on a blocking thread.
    match tokio::task::spawn_blocking(move || exec_zmemory(&codex_home, args)).await {
        Ok(Ok(result)) => success_result(result.text, result.structured_content),
        Ok(Err(err)) => error_result(format!("zmemory error: {err}")),
        Err(join_err) => error_result(format!("zmemory task panicked: {join_err}")),
    }
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

fn create_tool_input_schema(schema: schemars::schema::RootSchema) -> Arc<JsonObject> {
    #[expect(clippy::expect_used)]
    let schema_value = serde_json::to_value(&schema).expect("zmemory tool schema should serialize");
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

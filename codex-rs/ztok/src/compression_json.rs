use crate::compression::CompressionResult;
use crate::compression::ContentKind;
use crate::compression::ExplicitFallbackReason;
use crate::compression::JsonRenderMode;
use anyhow::Context;
use anyhow::Result;
use serde_json::Value;

pub(crate) fn compress_json(
    content: &str,
    content_kind: ContentKind,
    max_depth: usize,
    mode: JsonRenderMode,
) -> Result<CompressionResult> {
    if content_kind != ContentKind::Json {
        return Ok(CompressionResult::fallback_full(
            content_kind,
            content.to_string(),
            ExplicitFallbackReason::StrategyUnavailable,
        ));
    }

    let output = match mode {
        JsonRenderMode::Compact => {
            let value: Value = serde_json::from_str(content).context("解析 JSON 失败")?;
            compact_json(&value, /*depth*/ 0, max_depth)
        }
        JsonRenderMode::Schema => {
            let value: Value = serde_json::from_str(content).context("解析 JSON 失败")?;
            extract_schema(&value, /*depth*/ 0, max_depth)
        }
        JsonRenderMode::Summary => match serde_json::from_str::<Value>(content) {
            Ok(value) => summarize_json_value(&value),
            Err(_) => "   （JSON 无效）".to_string(),
        },
    };

    Ok(CompressionResult::full(content_kind, output))
}

fn compact_json(value: &Value, depth: usize, max_depth: usize) -> String {
    let indent = "  ".repeat(depth);

    if depth > max_depth {
        return format!("{indent}...");
    }

    match value {
        Value::Null => format!("{indent}null"),
        Value::Bool(flag) => format!("{indent}{flag}"),
        Value::Number(number) => format!("{indent}{number}"),
        Value::String(text) => {
            if text.chars().count() > 80 {
                let truncated = text.chars().take(77).collect::<String>();
                format!("{indent}\"{truncated}...\"")
            } else {
                format!("{indent}\"{text}\"")
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                format!("{indent}[]")
            } else if arr.len() > 5 {
                let first = compact_json(&arr[0], depth + 1, max_depth);
                format!("{}[{}, ... +{} more]", indent, first.trim(), arr.len() - 1)
            } else {
                let items: Vec<String> = arr
                    .iter()
                    .map(|item| compact_json(item, depth + 1, max_depth))
                    .collect();
                let all_simple = arr.iter().all(|item| {
                    matches!(
                        item,
                        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
                    )
                });
                if all_simple {
                    let inline: Vec<&str> = items.iter().map(|item| item.trim()).collect();
                    format!("{}[{}]", indent, inline.join(", "))
                } else {
                    let mut lines = vec![format!("{}[", indent)];
                    for item in &items {
                        lines.push(format!("{item},"));
                    }
                    lines.push(format!("{indent}]"));
                    lines.join("\n")
                }
            }
        }
        Value::Object(map) => {
            if map.is_empty() {
                format!("{indent}{{}}")
            } else {
                let mut lines = vec![format!("{}{{", indent)];
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();

                for (index, key) in keys.iter().enumerate() {
                    let item = &map[*key];
                    let is_simple = matches!(
                        item,
                        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
                    );

                    if is_simple {
                        let value_str = compact_json(item, /*depth*/ 0, max_depth);
                        lines.push(format!("{}  {}: {}", indent, key, value_str.trim()));
                    } else {
                        lines.push(format!("{indent}  {key}:"));
                        lines.push(compact_json(item, depth + 1, max_depth));
                    }

                    if index >= 20 {
                        let remaining_keys = keys.len() - index - 1;
                        if remaining_keys > 0 {
                            lines.push(format!("{indent}  ... +{remaining_keys} 个键"));
                        }
                        break;
                    }
                }
                lines.push(format!("{indent}}}"));
                lines.join("\n")
            }
        }
    }
}

fn extract_schema(value: &Value, depth: usize, max_depth: usize) -> String {
    let indent = "  ".repeat(depth);

    if depth > max_depth {
        return format!("{indent}...");
    }

    match value {
        Value::Null => format!("{indent}null"),
        Value::Bool(_) => format!("{indent}bool"),
        Value::Number(number) => {
            if number.is_i64() {
                format!("{indent}int")
            } else {
                format!("{indent}float")
            }
        }
        Value::String(text) => {
            if text.len() > 50 {
                format!("{}string[{}]", indent, text.len())
            } else if text.is_empty() {
                format!("{indent}string")
            } else if text.starts_with("http") {
                format!("{indent}url")
            } else if text.contains('-') && text.len() == 10 {
                format!("{indent}date?")
            } else {
                format!("{indent}string")
            }
        }
        Value::Array(arr) => {
            if arr.is_empty() {
                format!("{indent}[]")
            } else {
                let first_schema = extract_schema(&arr[0], depth + 1, max_depth);
                let trimmed = first_schema.trim();
                if arr.len() == 1 {
                    format!("{indent}[\n{first_schema}\n{indent}]")
                } else {
                    format!("{}[{}] ({})", indent, trimmed, arr.len())
                }
            }
        }
        Value::Object(map) => {
            if map.is_empty() {
                format!("{indent}{{}}")
            } else {
                let mut lines = vec![format!("{}{{", indent)];
                let mut keys: Vec<_> = map.keys().collect();
                keys.sort();

                for (index, key) in keys.iter().enumerate() {
                    let item = &map[*key];
                    let value_schema = extract_schema(item, depth + 1, max_depth);
                    let value_trimmed = value_schema.trim();
                    let is_simple = matches!(
                        item,
                        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
                    );

                    if is_simple {
                        if index < keys.len() - 1 {
                            lines.push(format!("{indent}  {key}: {value_trimmed},"));
                        } else {
                            lines.push(format!("{indent}  {key}: {value_trimmed}"));
                        }
                    } else {
                        lines.push(format!("{indent}  {key}:"));
                        lines.push(value_schema);
                    }

                    if index >= 15 {
                        lines.push(format!("{}  ... +{} 个键", indent, keys.len() - index - 1));
                        break;
                    }
                }
                lines.push(format!("{indent}}}"));
                lines.join("\n")
            }
        }
    }
}

fn summarize_json_value(value: &Value) -> String {
    let mut result = vec!["   JSON 输出：".to_string()];
    match value {
        Value::Array(arr) => result.push(format!("   数组，共 {} 项", arr.len())),
        Value::Object(obj) => {
            result.push(format!("   对象，共 {} 个键：", obj.len()));
            for key in obj.keys().take(10) {
                result.push(format!("   • {key}"));
            }
            if obj.len() > 10 {
                result.push(format!("   ... +{} 个键", obj.len() - 10));
            }
        }
        _ => result.push(format!(
            "   {}",
            crate::utils::truncate(&value.to_string(), /*max_len*/ 100)
        )),
    }
    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::CompressionHint;
    use crate::compression::CompressionIntent;
    use crate::compression::CompressionRequest;

    #[test]
    fn json_compact_keeps_values() {
        let result = crate::compression::compress(CompressionRequest {
            source_name: "payload.json",
            content: r#"{"token":"secret","count":2}"#,
            hint: CompressionHint::Json,
            intent: CompressionIntent::Json {
                max_depth: 5,
                mode: JsonRenderMode::Compact,
            },
        })
        .expect("json compact should succeed");

        assert_eq!(result.content_kind, ContentKind::Json);
        assert!(result.output.contains(r#"token: "secret""#));
        assert!(result.output.contains("count: 2"));
    }

    #[test]
    fn json_schema_limits_keys() {
        let json = serde_json::json!({
            "k00": 0, "k01": 1, "k02": 2, "k03": 3,
            "k04": 4, "k05": 5, "k06": 6, "k07": 7,
            "k08": 8, "k09": 9, "k10": 10, "k11": 11,
            "k12": 12, "k13": 13, "k14": 14, "k15": 15,
            "k16": 16
        });

        let result = crate::compression::compress(CompressionRequest {
            source_name: "payload.json",
            content: &json.to_string(),
            hint: CompressionHint::Json,
            intent: CompressionIntent::Json {
                max_depth: 5,
                mode: JsonRenderMode::Schema,
            },
        })
        .expect("json schema should succeed");

        assert!(result.output.contains("... +1 个键"));
    }
}

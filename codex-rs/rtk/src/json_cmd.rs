use crate::tracking;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use serde_json::Value;
use std::fs;
use std::io::Read;
use std::io::{self};
use std::path::Path;

/// 在执行 I/O 前明确拒绝非 JSON 文件。
fn validate_json_extension(file: &Path) -> Result<()> {
    if let Some(ext) = file.extension().and_then(|e| e.to_str()) {
        let format_name = match ext {
            "toml" => Some("TOML"),
            "yaml" | "yml" => Some("YAML"),
            "xml" => Some("XML"),
            "csv" => Some("CSV"),
            "ini" => Some("INI"),
            "env" => Some("env"),
            "txt" => Some("纯文本"),
            _ => None,
        };
        if let Some(fmt) = format_name {
            let mut msg = format!(
                "{} 不是 JSON 文件（检测到 {}）。非 JSON 文件请使用 `rtk read`。",
                file.display(),
                fmt
            );
            if ext == "toml" && file.file_name().is_some_and(|n| n == "Cargo.toml") {
                msg.push_str(" 提示：处理 Cargo.toml 可使用 `rtk deps`。");
            }
            bail!("{msg}");
        }
    }
    Ok(())
}

/// 展示不含具体值的 JSON 结构
pub fn run(file: &Path, max_depth: usize, verbose: u8) -> Result<()> {
    validate_json_extension(file)?;
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("分析 JSON：{}", file.display());
    }

    let content =
        fs::read_to_string(file).with_context(|| format!("读取文件失败：{}", file.display()))?;

    let schema = filter_json_string(&content, max_depth)?;
    println!("{schema}");
    timer.track(
        &format!("cat {}", file.display()),
        "rtk json",
        &content,
        &schema,
    );
    Ok(())
}

/// 展示来自 stdin 的 JSON 结构
pub fn run_stdin(max_depth: usize, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("分析 stdin 的 JSON");
    }

    let mut content = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut content)
        .context("从 stdin 读取失败")?;

    let schema = filter_json_string(&content, max_depth)?;
    println!("{schema}");
    timer.track("cat - (stdin)", "rtk json -", &content, &schema);
    Ok(())
}

/// 解析 JSON 字符串并返回其结构表示。
/// 适用于管道输入的 JSON（例如 `gh api`、`curl`）。
pub fn filter_json_string(json_str: &str, max_depth: usize) -> Result<String> {
    let value: Value = serde_json::from_str(json_str).context("解析 JSON 失败")?;
    Ok(extract_schema(&value, /*depth*/ 0, max_depth))
}

fn extract_schema(value: &Value, depth: usize, max_depth: usize) -> String {
    let indent = "  ".repeat(depth);

    if depth > max_depth {
        return format!("{indent}...");
    }

    match value {
        Value::Null => format!("{indent}null"),
        Value::Bool(_) => format!("{indent}bool"),
        Value::Number(n) => {
            if n.is_i64() {
                format!("{indent}int")
            } else {
                format!("{indent}float")
            }
        }
        Value::String(s) => {
            if s.len() > 50 {
                format!("{}string[{}]", indent, s.len())
            } else if s.is_empty() {
                format!("{indent}string")
            } else {
                // Check if it looks like a URL, date, etc.
                if s.starts_with("http") {
                    format!("{indent}url")
                } else if s.contains('-') && s.len() == 10 {
                    format!("{indent}date?")
                } else {
                    format!("{indent}string")
                }
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

                for (i, key) in keys.iter().enumerate() {
                    let val = &map[*key];
                    let val_schema = extract_schema(val, depth + 1, max_depth);
                    let val_trimmed = val_schema.trim();

                    // 简单类型直接内联显示
                    let is_simple = matches!(
                        val,
                        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_)
                    );

                    if is_simple {
                        if i < keys.len() - 1 {
                            lines.push(format!("{indent}  {key}: {val_trimmed},"));
                        } else {
                            lines.push(format!("{indent}  {key}: {val_trimmed}"));
                        }
                    } else {
                        lines.push(format!("{indent}  {key}:"));
                        lines.push(val_schema);
                    }

                    // 限制展示的键数量
                    if i >= 15 {
                        lines.push(format!("{}  ... +{} 个键", indent, keys.len() - i - 1));
                        break;
                    }
                }
                lines.push(format!("{indent}}}"));
                lines.join("\n")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- #347: validate_json_extension ---

    #[test]
    fn test_toml_file_rejected() {
        let err = validate_json_extension(Path::new("config.toml")).unwrap_err();
        assert!(err.to_string().contains("不是 JSON 文件"));
        assert!(err.to_string().contains("TOML"));
    }

    #[test]
    fn test_cargo_toml_suggests_deps() {
        let err = validate_json_extension(Path::new("Cargo.toml")).unwrap_err();
        assert!(err.to_string().contains("rtk deps"));
    }

    #[test]
    fn test_yaml_file_rejected() {
        let err = validate_json_extension(Path::new("config.yaml")).unwrap_err();
        assert!(err.to_string().contains("YAML"));
    }

    #[test]
    fn test_json_file_accepted() {
        assert!(validate_json_extension(Path::new("data.json")).is_ok());
    }

    #[test]
    fn test_unknown_extension_accepted() {
        assert!(validate_json_extension(Path::new("data.xyz")).is_ok());
    }

    #[test]
    fn test_no_extension_accepted() {
        assert!(validate_json_extension(Path::new("Makefile")).is_ok());
    }

    #[test]
    fn test_extract_schema_simple() {
        let json: Value = serde_json::from_str(r#"{"name": "test", "count": 42}"#).unwrap();
        let schema = extract_schema(&json, 0, 5);
        assert!(schema.contains("name"));
        assert!(schema.contains("string"));
        assert!(schema.contains("int"));
    }

    #[test]
    fn test_extract_schema_array() {
        let json: Value = serde_json::from_str(r#"{"items": [1, 2, 3]}"#).unwrap();
        let schema = extract_schema(&json, 0, 5);
        assert!(schema.contains("items"));
        assert!(schema.contains("(3)"));
    }

    #[test]
    fn test_extract_schema_limits_key_count_in_chinese() {
        let json = serde_json::json!({
            "k00": 0, "k01": 1, "k02": 2, "k03": 3,
            "k04": 4, "k05": 5, "k06": 6, "k07": 7,
            "k08": 8, "k09": 9, "k10": 10, "k11": 11,
            "k12": 12, "k13": 13, "k14": 14, "k15": 15,
            "k16": 16
        });

        let schema = extract_schema(&json, 0, 5);
        assert!(schema.contains("... +1 个键"), "实际得到：{schema}");
    }
}

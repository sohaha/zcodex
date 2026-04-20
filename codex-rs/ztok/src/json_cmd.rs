use crate::compression;
use crate::compression::CompressionHint;
use crate::compression::CompressionIntent;
use crate::compression::CompressionRequest;
use crate::compression::JsonRenderMode;
use crate::tracking;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use std::fs;
use std::io::Read;
use std::io::{self};
use std::path::Path;

/// 在执行输入/输出前明确拒绝非 JSON 文件。
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
                "{} 不是 JSON 文件（检测到 {}）。非 JSON 文件请使用 `ztok read`。",
                file.display(),
                fmt
            );
            if ext == "toml" && file.file_name().is_some_and(|n| n == "Cargo.toml") {
                msg.push_str(" 提示：处理 Cargo.toml 可使用 `ztok deps`。");
            }
            bail!("{msg}");
        }
    }
    Ok(())
}

/// 展示 JSON：默认保留简化值，或通过 `schema_only` 仅展示键和类型。
pub fn run(file: &Path, max_depth: usize, schema_only: bool, verbose: u8) -> Result<()> {
    validate_json_extension(file)?;
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("分析 JSON: {}", file.display());
    }

    let content =
        fs::read_to_string(file).with_context(|| format!("读取文件失败：{}", file.display()))?;

    let source_name = file.display().to_string();
    let output = compression::compress(CompressionRequest {
        source_name: &source_name,
        content: &content,
        hint: CompressionHint::Json,
        intent: CompressionIntent::Json {
            max_depth,
            mode: if schema_only {
                JsonRenderMode::Schema
            } else {
                JsonRenderMode::Compact
            },
        },
    })?
    .output;
    println!("{output}");
    timer.track(
        &format!("cat {}", file.display()),
        "ztok json",
        &content,
        &output,
    );
    Ok(())
}

/// 展示来自标准输入的 JSON。
pub fn run_stdin(max_depth: usize, schema_only: bool, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("分析标准输入中的 JSON");
    }

    let mut content = String::new();
    io::stdin()
        .lock()
        .read_to_string(&mut content)
        .context("从标准输入读取失败")?;

    let output = compression::compress(CompressionRequest {
        source_name: "-",
        content: &content,
        hint: CompressionHint::Json,
        intent: CompressionIntent::Json {
            max_depth,
            mode: if schema_only {
                JsonRenderMode::Schema
            } else {
                JsonRenderMode::Compact
            },
        },
    })?
    .output;
    println!("{output}");
    timer.track("cat - (stdin)", "ztok json -", &content, &output);
    Ok(())
}

/// 解析 JSON 字符串并返回保留值的紧凑表示。
pub fn filter_json_compact(json_str: &str, max_depth: usize) -> Result<String> {
    Ok(compression::compress(CompressionRequest {
        source_name: "-",
        content: json_str,
        hint: CompressionHint::Json,
        intent: CompressionIntent::Json {
            max_depth,
            mode: JsonRenderMode::Compact,
        },
    })?
    .output)
}

/// 解析 JSON 字符串并返回其结构表示。
/// 适用于管道输入的 JSON（例如 `gh api`、`curl`）。
pub fn filter_json_string(json_str: &str, max_depth: usize) -> Result<String> {
    Ok(compression::compress(CompressionRequest {
        source_name: "-",
        content: json_str,
        hint: CompressionHint::Json,
        intent: CompressionIntent::Json {
            max_depth,
            mode: JsonRenderMode::Schema,
        },
    })?
    .output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

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
        assert!(err.to_string().contains("ztok deps"));
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
        let schema = filter_json_string(&json.to_string(), 5).unwrap();
        assert!(schema.contains("name"));
        assert!(schema.contains("string"));
        assert!(schema.contains("int"));
    }

    #[test]
    fn test_extract_schema_array() {
        let json: Value = serde_json::from_str(r#"{"items": [1, 2, 3]}"#).unwrap();
        let schema = filter_json_string(&json.to_string(), 5).unwrap();
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

        let schema = filter_json_string(&json.to_string(), 5).unwrap();
        assert!(schema.contains("... +1 个键"), "实际得到：{schema}");
    }

    #[test]
    fn test_filter_json_compact_preserves_values() {
        let compact = filter_json_compact(r#"{"token":"secret","count":2}"#, 5).unwrap();
        assert!(
            compact.contains(r#"token: "secret""#),
            "实际得到：{compact}"
        );
        assert!(compact.contains("count: 2"), "实际得到：{compact}");
    }

    #[test]
    fn test_filter_json_compact_truncates_utf8_safely() {
        let compact =
            filter_json_compact(&format!(r#"{{"msg":"{}"}}"#, "界".repeat(90)), 5).unwrap();
        assert!(compact.contains(r#"msg: ""#), "实际得到：{compact}");
        assert!(compact.contains("..."), "实际得到：{compact}");
    }

    #[test]
    fn test_filter_json_compact_does_not_report_zero_remaining_keys() {
        let json = serde_json::json!({
            "k01": 1, "k02": 2, "k03": 3, "k04": 4, "k05": 5,
            "k06": 6, "k07": 7, "k08": 8, "k09": 9, "k10": 10,
            "k11": 11, "k12": 12, "k13": 13, "k14": 14, "k15": 15,
            "k16": 16, "k17": 17, "k18": 18, "k19": 19, "k20": 20,
            "k21": 21
        });

        let compact = filter_json_compact(&json.to_string(), 5).unwrap();
        assert!(!compact.contains("... +0 个键"), "实际得到：{compact}");
    }
}

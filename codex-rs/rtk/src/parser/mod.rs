//! 用于工具输出转换的解析器基础设施。
//!
//! 本模块为工具输出解析提供统一接口，并支持可感知的降级处理：
//! - 第 1 层（Full）：完整 JSON 解析，保留全部字段
//! - 第 2 层（Degraded）：部分解析并附带警告
//! - 第 3 层（Passthrough）：截断原始输出并附带错误标记
//!
//! 三层体系可确保 RTK 不会在无提示的情况下返回错误数据。

pub mod error;
pub mod formatter;
pub mod types;

pub use formatter::FormatMode;
pub use formatter::TokenFormatter;
pub use types::*;

/// 带降级层级的解析结果
#[derive(Debug)]
pub enum ParseResult<T> {
    /// 第 1 层：完整解析，包含全部结构化数据
    Full(T),

    /// 第 2 层：降级解析，包含部分数据和警告
    Degraded(T, Vec<String>),

    /// 第 3 层：直通原始输出，解析失败时返回截断后的原文
    Passthrough(String),
}

impl<T> ParseResult<T> {
    /// 取出解析后的数据；若为 `Passthrough` 则触发 panic
    pub fn unwrap(self) -> T {
        match self {
            ParseResult::Full(data) => data,
            ParseResult::Degraded(data, _) => data,
            ParseResult::Passthrough(_) => panic!("对 Passthrough 结果调用了 unwrap"),
        }
    }

    /// 获取层级编号（1 = `Full`，2 = `Degraded`，3 = `Passthrough`）
    pub fn tier(&self) -> u8 {
        match self {
            ParseResult::Full(_) => 1,
            ParseResult::Degraded(_, _) => 2,
            ParseResult::Passthrough(_) => 3,
        }
    }

    /// 检查解析是否成功（`Full` 或 `Degraded`）
    pub fn is_ok(&self) -> bool {
        !matches!(self, ParseResult::Passthrough(_))
    }

    /// 在保留层级信息的前提下映射解析结果
    pub fn map<U, F>(self, f: F) -> ParseResult<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            ParseResult::Full(data) => ParseResult::Full(f(data)),
            ParseResult::Degraded(data, warnings) => ParseResult::Degraded(f(data), warnings),
            ParseResult::Passthrough(raw) => ParseResult::Passthrough(raw),
        }
    }

    /// 若为 Degraded 层级，返回其中的警告
    pub fn warnings(&self) -> Vec<String> {
        match self {
            ParseResult::Degraded(_, warnings) => warnings.clone(),
            _ => vec![],
        }
    }
}

/// 工具输出的统一解析器 trait
pub trait OutputParser: Sized {
    type Output;

    /// 将原始输出解析为结构化格式
    ///
    /// 实现应遵循三层回退策略：
    /// 1. 先尝试 JSON 解析（若工具支持 `--json` / `--format json`）
    /// 2. 再尝试通过正则或文本提取部分数据
    /// 3. 最后返回带 `[RTK:PASSTHROUGH]` 标记的截断原始输出
    fn parse(input: &str) -> ParseResult<Self::Output>;

    /// 按指定最大层级解析（用于测试或调试）
    fn parse_with_tier(input: &str, max_tier: u8) -> ParseResult<Self::Output> {
        let result = Self::parse(input);
        if result.tier() > max_tier {
            // 若超过允许层级，则强制退化为 passthrough。
            return ParseResult::Passthrough(truncate_output(input, /*max_chars*/ 500));
        }
        result
    }
}

/// 将输出截断到最大长度，并在末尾附加提示
pub fn truncate_output(output: &str, max_chars: usize) -> String {
    let chars: Vec<char> = output.chars().collect();
    if chars.len() <= max_chars {
        return output.to_string();
    }

    let truncated: String = chars[..max_chars].iter().collect();
    format!(
        "{}\n\n[RTK:PASSTHROUGH] 输出已截断（{} 字符 → {} 字符）",
        truncated,
        chars.len(),
        max_chars
    )
}

/// 输出降级警告的辅助函数
pub fn emit_degradation_warning(tool: &str, reason: &str) {
    eprintln!("[RTK:DEGRADED] {tool} 解析器：{reason}");
}

/// 输出 passthrough 警告的辅助函数
pub fn emit_passthrough_warning(tool: &str, reason: &str) {
    eprintln!("[RTK:PASSTHROUGH] {tool} 解析器：{reason}");
}

/// 从可能带有非 JSON 前缀的输入中提取完整 JSON 对象
/// （例如 pnpm 横幅、dotenv 提示等）。
///
/// 策略：
/// 1. 查找 `"numTotalTests"`（vitest 专用标记）或第一个独立出现的 `{`
/// 2. 向前进行花括号配平，找到匹配的 `}`
/// 3. 返回完整 JSON 对象所在的切片
///
/// 支持场景：嵌套花括号、字符串转义、pnpm 前缀、dotenv 横幅
///
/// 若未找到有效 JSON 对象，则返回 `None`。
pub fn extract_json_object(input: &str) -> Option<&str> {
    // 先尝试 vitest 专用标记（最可靠）。
    let start_pos = if let Some(pos) = input.find("\"numTotalTests\"") {
        // 向后回溯，找到对象的起始花括号。
        input[..pos].rfind('{').unwrap_or(0)
    } else {
        // 回退方案：寻找独占一行或位于空白后的第一个 `{`。
        let mut found_start = None;
        for (idx, line) in input.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('{') {
                // 计算字节偏移量。
                found_start = Some(
                    input[..]
                        .lines()
                        .take(idx)
                        .map(|l| l.len() + 1)
                        .sum::<usize>(),
                );
                break;
            }
        }
        found_start?
    };

    // 从起始位置向前做花括号配平。
    let mut depth = 0;
    let mut in_string = false;
    let mut escape_next = false;
    let chars: Vec<char> = input[start_pos..].chars().collect();

    for (i, &ch) in chars.iter().enumerate() {
        if escape_next {
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if in_string => escape_next = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    // 找到匹配的结束花括号。
                    let end_pos = start_pos + i + 1; // +1 以包含 `}`
                    return Some(&input[start_pos..end_pos]);
                }
            }
            _ => {}
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_result_tier() {
        let full: ParseResult<i32> = ParseResult::Full(42);
        assert_eq!(full.tier(), 1);
        assert!(full.is_ok());

        let degraded: ParseResult<i32> = ParseResult::Degraded(42, vec!["warning".to_string()]);
        assert_eq!(degraded.tier(), 2);
        assert!(degraded.is_ok());
        assert_eq!(degraded.warnings().len(), 1);

        let passthrough: ParseResult<i32> = ParseResult::Passthrough("raw".to_string());
        assert_eq!(passthrough.tier(), 3);
        assert!(!passthrough.is_ok());
    }

    #[test]
    fn test_parse_result_map() {
        let full: ParseResult<i32> = ParseResult::Full(42);
        let mapped = full.map(|x| x * 2);
        assert_eq!(mapped.tier(), 1);
        assert_eq!(mapped.unwrap(), 84);

        let degraded: ParseResult<i32> = ParseResult::Degraded(42, vec!["warn".to_string()]);
        let mapped = degraded.map(|x| x * 2);
        assert_eq!(mapped.tier(), 2);
        assert_eq!(mapped.warnings().len(), 1);
        assert_eq!(mapped.unwrap(), 84);
    }

    #[test]
    fn test_truncate_output() {
        let short = "hello";
        assert_eq!(truncate_output(short, 10), "hello");

        let long = "a".repeat(1000);
        let truncated = truncate_output(&long, 100);
        assert!(truncated.contains("[RTK:PASSTHROUGH]"));
        assert!(truncated.contains("1000 字符 → 100 字符"));
    }

    #[test]
    fn test_truncate_output_multibyte() {
        // 泰文：每个字符约 3 字节
        let thai = "สวัสดีครับ".repeat(100);
        // 尝试在可能落在字符中间的偏移处截断
        let result = truncate_output(&thai, 50);
        assert!(result.contains("[RTK:PASSTHROUGH]"));
        // 应保持有效 UTF-8（不能 panic）
        let _ = result.len();
    }

    #[test]
    fn test_truncate_output_emoji() {
        let emoji = "🎉".repeat(200);
        let result = truncate_output(&emoji, 100);
        assert!(result.contains("[RTK:PASSTHROUGH]"));
    }

    #[test]
    fn test_extract_json_object_clean() {
        let input = r#"{"numTotalTests": 13, "numPassedTests": 13}"#;
        let extracted = extract_json_object(input);
        assert_eq!(extracted, Some(input));
    }

    #[test]
    fn test_extract_json_object_with_pnpm_prefix() {
        let input = r#"
Scope: all 6 workspace projects
 WARN  deprecated inflight@1.0.6: This module is not supported

{"numTotalTests": 13, "numPassedTests": 13, "numFailedTests": 0}
"#;
        let extracted = extract_json_object(input).expect("应成功提取 JSON");
        assert!(extracted.contains("numTotalTests"));
        assert!(extracted.starts_with('{'));
        assert!(extracted.ends_with('}'));
    }

    #[test]
    fn test_extract_json_object_with_dotenv_prefix() {
        let input = r#"[dotenv] Loading environment variables from .env
[dotenv] Injected 5 variables

{"numTotalTests": 5, "testResults": [{"name": "test.js"}]}
"#;
        let extracted = extract_json_object(input).expect("应成功提取 JSON");
        assert!(extracted.contains("numTotalTests"));
        assert!(extracted.contains("testResults"));
    }

    #[test]
    fn test_extract_json_object_nested_braces() {
        let input = r#"prefix text
{"numTotalTests": 2, "testResults": [{"name": "test", "data": {"nested": true}}]}
"#;
        let extracted = extract_json_object(input).expect("应成功提取 JSON");
        assert!(extracted.contains("\"nested\": true"));
        assert!(extracted.starts_with('{'));
        assert!(extracted.ends_with('}'));
    }

    #[test]
    fn test_extract_json_object_no_json() {
        let input = "Just plain text with no JSON";
        let extracted = extract_json_object(input);
        assert_eq!(extracted, None);
    }

    #[test]
    fn test_extract_json_object_string_with_braces() {
        let input = r#"{"numTotalTests": 1, "message": "test {should} not confuse parser"}"#;
        let extracted = extract_json_object(input).expect("应成功提取 JSON");
        assert!(extracted.contains("test {should} not confuse parser"));
        assert_eq!(extracted, input);
    }
}

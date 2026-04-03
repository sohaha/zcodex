# 解析器基础设施

## 概览

解析器基础设施为工具输出提供统一的三层解析体系，并支持可感知的降级处理：

- **Tier 1（Full）**：完整 JSON 解析，保留全部结构化数据
- **Tier 2（Degraded）**：部分解析并附带警告（回退到正则提取）
- **Tier 3（Passthrough）**：截断原始输出并标记解析错误

这样可以确保 ZTOK **不会在无提示的情况下返回错误数据**，同时尽量保持 token 效率。

## 架构

```
┌─────────────────────────────────────────────────────────┐
│                    ToolCommand 构建器                   │
│  Command::new("vitest").arg("--reporter=json")          │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                   OutputParser<T> Trait                  │
│  parse() → ParseResult<T>                               │
│    ├─ Full(T)           - Tier 1：完整 JSON 解析       │
│    ├─ Degraded(T, warn) - Tier 2：部分解析并带警告     │
│    └─ Passthrough(str)  - Tier 3：截断原始输出         │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                     标准类型                             │
│  TestResult, LintResult, DependencyState, BuildOutput   │
└─────────────────────┬───────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────┐
│                  TokenFormatter Trait                    │
│  format_compact() / format_verbose() / format_ultra()   │
└─────────────────────────────────────────────────────────┘
```

## 使用示例

### 1. 定义工具专用解析器

```rust
use crate::parser::{OutputParser, ParseResult, TestResult};

struct VitestParser;

impl OutputParser for VitestParser {
    type Output = TestResult;

    fn parse(input: &str) -> ParseResult<TestResult> {
        // Tier 1：尝试 JSON 解析
        match serde_json::from_str::<VitestJsonOutput>(input) {
            Ok(json) => {
                let result = TestResult {
                    total: json.num_total_tests,
                    passed: json.num_passed_tests,
                    failed: json.num_failed_tests,
                    // ... 映射字段
                };
                ParseResult::Full(result)
            }
            Err(e) => {
                // Tier 2：尝试正则提取
                if let Some(stats) = extract_stats_regex(input) {
                    ParseResult::Degraded(
                        stats,
                        vec![format!("JSON 解析失败：{}", e)]
                    )
                } else {
                    // Tier 3：直通原始输出
                    ParseResult::Passthrough(truncate_output(input, 500))
                }
            }
        }
    }
}
```

### 2. 在命令模块中使用解析器

```rust
use crate::parser::{OutputParser, TokenFormatter, FormatMode};

pub fn run_vitest(args: &[String], verbose: u8) -> Result<()> {
    let mut cmd = Command::new("pnpm");
    cmd.arg("vitest").arg("--reporter=json");
    // ... 添加参数

    let output = cmd.output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);

    // 解析输出
    let result = VitestParser::parse(&stdout);

    // 按详细级别格式化
    let mode = FormatMode::from_verbosity(verbose);
    let formatted = match result {
        ParseResult::Full(data) => data.format(mode),
        ParseResult::Degraded(data, warnings) => {
            if verbose > 0 {
                for warn in warnings {
                    eprintln!("[ZTOK:DEGRADED] {warn}");
                }
            }
            data.format(mode)
        }
        ParseResult::Passthrough(raw) => {
            eprintln!("[ZTOK:PASSTHROUGH] 解析失败，显示截断后的输出");
            raw
        }
    };

    println!("{formatted}");
    Ok(())
}
```

## 标准类型

### TestResult
适用于测试运行器（vitest、playwright、jest 等）
- 字段：`total`、`passed`、`failed`、`skipped`、`duration_ms`、`failures`
- 格式化：显示摘要和失败详情（compact 显示前 5 条，verbose 显示全部）

### LintResult
适用于 linter（eslint、biome、tsc 等）
- 字段：`total_files`、`files_with_issues`、`total_issues`、`errors`、`warnings`、`issues`
- 格式化：按 `rule_id` 分组，显示高频违规项

### DependencyState
适用于包管理器（pnpm、npm、cargo 等）
- 字段：`total_packages`、`outdated_count`、`dependencies`
- 格式化：显示升级路径（current → latest）

### BuildOutput
适用于构建工具（next、webpack、vite、cargo 等）
- 字段：`success`、`duration_ms`、`bundles`、`routes`、`warnings`、`errors`
- 格式化：显示 bundle 大小与路由指标

## 格式模式

### Compact（默认，verbosity=0）
- 仅显示摘要
- 显示前 5-10 项
- 面向 token 优化

### Verbose（verbosity=1）
- 显示完整细节
- 显示全部项目（最多 20 项）
- 更适合人工阅读

### Ultra（verbosity=2+）
- 使用符号：✓✗⚠📦⬆️
- 极致压缩
- 减少约 30-50% token

## 错误处理

### ParseError 类型
- `JsonError`：包含行/列上下文，便于调试
- `PatternMismatch`：正则模式匹配失败
- `PartialParse`：部分字段缺失
- `InvalidFormat`：结构不符合预期
- `MissingField`：缺少必填字段
- `VersionMismatch`：工具版本不兼容
- `EmptyOutput`：没有可解析的数据

### 降级警告

```
[ZTOK:DEGRADED] vitest parser: 第 42 行 JSON 解析失败，已回退到正则提取
[ZTOK:PASSTHROUGH] playwright parser: 模式不匹配，显示截断后的输出
```

## 迁移指南

### 现有模块 → Parser Trait

**修改前：**
```rust
fn run_vitest(args: &[String]) -> Result<()> {
    let output = Command::new("vitest").output()?;
    let filtered = filter_vitest_output(&output.stdout);
    println!("{filtered}");
    Ok(())
}
```

**修改后：**
```rust
fn run_vitest(args: &[String], verbose: u8) -> Result<()> {
    let output = Command::new("vitest")
        .arg("--reporter=json")
        .output()?;

    let result = VitestParser::parse(&output.stdout);
    let mode = FormatMode::from_verbosity(verbose);

    match result {
        ParseResult::Full(data) | ParseResult::Degraded(data, _) => {
            println!("{}", data.format(mode));
        }
        ParseResult::Passthrough(raw) => {
            println!("{raw}");
        }
    }
    Ok(())
}
```

## 测试

### 单元测试
```bash
cargo test parser::tests
```

### 集成测试
```bash
# 使用真实工具输出进行测试
echo '{"testResults": [...]}' | cargo run -- vitest parse
```

### 层级验证
```rust
#[test]
fn test_vitest_json_parsing() {
    let json = include_str!("fixtures/vitest-v1.json");
    let result = VitestParser::parse(json);
    assert_eq!(result.tier(), 1); // 完整解析
    assert!(result.is_ok());
}

#[test]
fn test_vitest_regex_fallback() {
    let text = "Test Files  2 passed (2)\n Tests  13 passed (13)";
    let result = VitestParser::parse(text);
    assert_eq!(result.tier(), 2); // 降级解析
    assert!(!result.warnings().is_empty());
}
```

## 收益

1. **可维护性**：工具版本变化时可平滑降级（第 2/3 层回退）
2. **可靠性**：不会静默失败，也不会返回错误数据
3. **可观测性**：在 verbose 模式下可清晰看到降级标记
4. **Token 效率**：结构化数据更利于压缩
5. **一致性**：所有工具类型使用统一接口
6. **可测试性**：基于 fixture 的回归测试可覆盖多个版本

## 路线图

### 第 4 阶段：模块迁移
- [ ] vitest_cmd.rs → VitestParser
- [ ] playwright_cmd.rs → PlaywrightParser
- [ ] pnpm_cmd.rs → PnpmParser (list, outdated)
- [ ] lint_cmd.rs → EslintParser
- [ ] tsc_cmd.rs → TscParser
- [ ] gh_cmd.rs → GhParser

### 第 5 阶段：可观测性
- [ ] 扩展 `tracking.db`：加入 `parse_tier`、`format_mode`
- [ ] 增加 `ztok parse-health` 命令
- [ ] 当降级比例 > 10% 时告警

//! 文本处理与命令执行相关的工具函数。
//!
//! 为 RTK 各命令提供通用辅助能力：
//! - 去除 ANSI 颜色码
//! - 文本截断
//! - 带错误上下文的命令执行

use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use std::borrow::Cow;
use std::path::PathBuf;
use std::process::Command;

pub(crate) fn compile_regex(pattern: &str) -> Regex {
    match Regex::new(pattern) {
        Ok(regex) => regex,
        Err(err) => panic!("无效的正则模式 {pattern:?}: {err}"),
    }
}

/// 将字符串截断到 `max_len` 个字符，必要时追加 `...`。
///
/// # 参数
/// * `s` - 要截断的字符串
/// * `max_len` - 触发截断的最大长度（最小为 3，才能容纳 `...`）
///
/// # 示例
/// ```
/// use rtk::utils::truncate;
/// assert_eq!(truncate("hello world", 8), "hello...");
/// assert_eq!(truncate("hi", 10), "hi");
/// ```
pub fn truncate(s: &str, max_len: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_len {
        s.to_string()
    } else if max_len < 3 {
        // 若 max_len 太小，直接返回 "..."
        "...".to_string()
    } else {
        format!("{}...", s.chars().take(max_len - 3).collect::<String>())
    }
}

/// 从字符串中移除 ANSI 转义码（颜色、样式等）。
///
/// # 参数
/// * `text` - 可能包含 ANSI 转义码的文本
///
/// # 示例
/// ```
/// use rtk::utils::strip_ansi;
/// let colored = "\x1b[31mError\x1b[0m";
/// assert_eq!(strip_ansi(colored), "Error");
/// ```
pub fn strip_ansi(text: &str) -> String {
    lazy_static::lazy_static! {
        static ref ANSI_RE: Regex = compile_regex(r"\x1b\[[0-9;]*[a-zA-Z]");
    }
    ANSI_RE.replace_all(text, "").to_string()
}

/// 执行命令并返回清洗后的 stdout/stderr。
///
/// # 参数
/// * `cmd` - 要执行的命令（例如 `"eslint"`）
/// * `args` - 命令参数
///
/// # 返回值
/// `(stdout: String, stderr: String, exit_code: i32)`
///
/// # 示例
/// ```no_run
/// use rtk::utils::execute_command;
/// let (stdout, stderr, code) = execute_command("echo", &["test"]).unwrap();
/// assert_eq!(code, 0);
/// ```
#[allow(dead_code)]
pub fn execute_command(cmd: &str, args: &[&str]) -> Result<(String, String, i32)> {
    let output = resolved_command(cmd)
        .args(args)
        .output()
        .context(format!("执行命令失败：{cmd}"))?;

    let stdout = decode_output(&output.stdout).to_string();
    let stderr = decode_output(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok((stdout, stderr, exit_code))
}

/// 将进程输出字节解码为文本。
///
/// 优先使用 UTF-8。若字节流不是 UTF-8，在 Windows 上会先借助
/// `chardetng` 猜测传统编码（如 GBK/Shift-JIS/Windows-1252），
/// 最后再回退到有损 UTF-8 解码。
pub fn decode_output(bytes: &[u8]) -> Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Cow::Borrowed(text);
    }

    #[cfg(target_os = "windows")]
    {
        let mut detector = chardetng::EncodingDetector::new();
        detector.feed(bytes, true);
        let encoding = detector.guess(None, true);
        if let Some(decoded) = encoding.decode_without_bom_handling_and_without_replacement(bytes) {
            return decoded;
        }

        encoding.decode(bytes).0
    }

    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes)
    }
}

/// 将 token 数格式化为带 K/M 后缀的可读形式。
///
/// # 参数
/// * `n` - token 数量
///
/// # 返回值
/// 格式化后的字符串（例如 `"1.2M"`、`"59.2K"`、`"694"`）
///
/// # 示例
/// ```
/// use rtk::utils::format_tokens;
/// assert_eq!(format_tokens(1_234_567), "1.2M");
/// assert_eq!(format_tokens(59_234), "59.2K");
/// assert_eq!(format_tokens(694), "694");
/// ```
#[cfg_attr(not(test), allow(dead_code))]
pub fn format_tokens(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        format!("{n}")
    }
}

/// 将美元金额按自适应精度格式化。
///
/// # 参数
/// * `amount` - 美元金额
///
/// # 返回值
/// 带 `$` 前缀的格式化字符串
///
/// # 示例
/// ```
/// use rtk::utils::format_usd;
/// assert_eq!(format_usd(1234.567), "$1234.57");
/// assert_eq!(format_usd(12.345), "$12.35");
/// assert_eq!(format_usd(0.123), "$0.12");
/// assert_eq!(format_usd(0.0096), "$0.0096");
/// ```
#[cfg_attr(not(test), allow(dead_code))]
pub fn format_usd(amount: f64) -> String {
    if !amount.is_finite() {
        return "$0.00".to_string();
    }
    if amount >= 0.01 {
        format!("${amount:.2}")
    } else {
        format!("${amount:.4}")
    }
}

/// 将单 token 成本格式化为 `$ / MTok`（例如 `"$3.86/MTok"`）
///
/// # 参数
/// * `cpt` - 单 token 成本（不是每百万 token 成本）
///
/// # 返回值
/// 形如 `"$3.86/MTok"` 的格式化字符串
///
/// # 示例
/// ```
/// use rtk::utils::format_cpt;
/// assert_eq!(format_cpt(0.000003), "$3.00/MTok");
/// assert_eq!(format_cpt(0.0000038), "$3.80/MTok");
/// assert_eq!(format_cpt(0.00000386), "$3.86/MTok");
/// ```
#[cfg_attr(not(test), allow(dead_code))]
pub fn format_cpt(cpt: f64) -> String {
    if !cpt.is_finite() || cpt <= 0.0 {
        return "$0.00/MTok".to_string();
    }
    let cpt_per_million = cpt * 1_000_000.0;
    format!("${cpt_per_million:.2}/MTok")
}

/// 将条目拼成按行分隔的字符串；当 `total > max` 时追加溢出提示。
///
/// # 示例
/// ```
/// use rtk::utils::join_with_overflow;
/// let items = vec!["a".to_string(), "b".to_string()];
/// assert_eq!(join_with_overflow(&items, 5, 3, "条目"), "a\nb\n... +2 个条目");
/// assert_eq!(join_with_overflow(&items, 2, 3, "条目"), "a\nb");
/// ```
pub fn join_with_overflow(items: &[String], total: usize, max: usize, label: &str) -> String {
    let mut out = items.join("\n");
    if total > max {
        out.push_str(&format!("\n... +{} 个{}", total - max, label));
    }
    out
}

/// 将 ISO 8601 日期时间截断为仅保留日期部分（前 10 个字符）。
///
/// # 示例
/// ```
/// use rtk::utils::truncate_iso_date;
/// assert_eq!(truncate_iso_date("2024-01-15T10:30:00Z"), "2024-01-15");
/// assert_eq!(truncate_iso_date("2024-01-15"), "2024-01-15");
/// assert_eq!(truncate_iso_date("short"), "short");
/// ```
pub fn truncate_iso_date(date: &str) -> &str {
    if date.len() >= 10 { &date[..10] } else { date }
}

/// 格式化确认消息："已\<action\> \<detail\>"
/// 用于写操作（merge、create、comment、edit 等）
///
/// # 示例
/// ```
/// use rtk::utils::ok_confirmation;
/// assert_eq!(ok_confirmation("合并", "#42"), "已合并 #42");
/// assert_eq!(ok_confirmation("创建", "PR #5 https://..."), "已创建 PR #5 https://...");
/// ```
pub fn ok_confirmation(action: &str, detail: &str) -> String {
    if detail.is_empty() {
        format!("已{action}")
    } else {
        format!("已{action} {detail}")
    }
}

/// 检测当前目录使用的包管理器。
/// 根据 lockfile 判断，返回 `"pnpm"`、`"yarn"` 或 `"npm"`。
///
/// # 示例
/// ```no_run
/// use rtk::utils::detect_package_manager;
/// let pm = detect_package_manager();
/// // 若存在 pnpm-lock.yaml 返回 "pnpm"，存在 yarn.lock 返回 "yarn"，否则返回 "npm"
/// ```
#[allow(dead_code)]
pub fn detect_package_manager() -> &'static str {
    if std::path::Path::new("pnpm-lock.yaml").exists() {
        "pnpm"
    } else if std::path::Path::new("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}

/// 使用检测到的包管理器的 `exec` 机制构造 `Command`。
/// 返回值可继续追加工具专属参数。
pub fn package_manager_exec(tool: &str) -> Command {
    if tool_exists(tool) {
        resolved_command(tool)
    } else {
        let pm = detect_package_manager();
        match pm {
            "pnpm" => {
                let mut c = resolved_command("pnpm");
                c.arg("exec").arg("--").arg(tool);
                c
            }
            "yarn" => {
                let mut c = resolved_command("yarn");
                c.arg("exec").arg("--").arg(tool);
                c
            }
            _ => {
                let mut c = resolved_command("npx");
                c.arg("--no-install").arg("--").arg(tool);
                c
            }
        }
    }
}

/// 将二进制名解析为完整路径，并在 Windows 上遵循 `PATHEXT`。
///
/// 在 Windows 上，Node.js 工具通常以 `.CMD` / `.BAT` / `.PS1` 包装脚本形式安装。
/// Rust 的 `std::process::Command::new()` 不会遵循 `PATHEXT`，
/// 所以即使 `vitest.CMD` 在 PATH 中，`Command::new("vitest")` 也可能失败。
///
/// 本函数借助 `which` crate 做正确的 PATH + PATHEXT 解析。
///
/// # 参数
/// * `name` - 二进制名称（例如 `"vitest"`、`"eslint"`、`"tsc"`）
///
/// # 返回值
/// 解析后的完整路径；若未找到则返回错误。
pub fn resolve_binary(name: &str) -> Result<PathBuf> {
    which::which(name).context(format!("PATH 中未找到二进制 '{name}'"))
}

/// 创建一个支持 `PATHEXT` 解析的 `Command`。
///
/// 可作为 `Command::new(name)` 的直接替代，用于兼容 Windows 下的
/// `.CMD` / `.BAT` / `.PS1` 包装器。
///
/// 若解析失败，则回退到 `Command::new(name)`，保证原生命令
/// （如 `git`、`cargo`）在 `which` 找不到时仍可尝试运行。
///
/// # 参数
/// * `name` - 二进制名称（例如 `"vitest"`、`"eslint"`）
///
/// # 返回值
/// 已配置好解析后二进制路径的 `Command`。
pub fn resolved_command(name: &str) -> Command {
    let resolved = resolve_binary(name);

    #[cfg(target_os = "windows")]
    if let Err(error) = &resolved {
        eprintln!(
            "rtk：通过 PATH 解析 '{}' 失败，回退到直接执行：{}",
            name, error
        );
    }

    #[cfg(all(not(target_os = "windows"), debug_assertions))]
    if let Err(error) = &resolved {
        eprintln!("rtk：通过 PATH 解析 '{name}' 失败，回退到直接执行：{error}");
    }

    match resolved {
        Ok(path) => Command::new(path),
        Err(_) => Command::new(name),
    }
}

/// 检查某个工具是否存在于 PATH 中（Windows 上支持 PATHEXT）。
///
/// 可替代手写的 `Command::new("which").arg(tool)` 检查，避免其在 Windows 上失效。
pub fn tool_exists(name: &str) -> bool {
    which::which(name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        let result = truncate("hello world", 8);
        assert_eq!(result, "hello...");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_edge_case() {
        // 当 `max_len < 3` 时，只返回 `...`
        assert_eq!(truncate("hello", 2), "...");
        // 当字符串长度等于 `max_len` 时，原样返回
        assert_eq!(truncate("abc", 3), "abc");
        // 当字符串更长且 `max_len == 3` 时，返回 `...`
        assert_eq!(truncate("hello world", 3), "...");
    }

    #[test]
    fn test_strip_ansi_simple() {
        let input = "\x1b[31mError\x1b[0m";
        assert_eq!(strip_ansi(input), "Error");
    }

    #[test]
    fn test_strip_ansi_multiple() {
        let input = "\x1b[1m\x1b[32mSuccess\x1b[0m\x1b[0m";
        assert_eq!(strip_ansi(input), "Success");
    }

    #[test]
    fn test_decode_output_prefers_utf8() {
        let text = "中文 output";
        let decoded = decode_output(text.as_bytes());
        assert_eq!(decoded, text);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_decode_output_detects_gbk() {
        let gbk_bytes = [0xD6, 0xD0, 0xCE, 0xC4]; // "中文" in GBK
        let decoded = decode_output(&gbk_bytes);
        assert_eq!(decoded, "中文");
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn test_decode_output_non_utf8_falls_back_lossy() {
        let decoded = decode_output(&[0xD6, 0xD0, 0xCE, 0xC4]);
        assert!(decoded.contains('\u{fffd}'));
    }

    #[test]
    fn test_strip_ansi_no_codes() {
        assert_eq!(strip_ansi("plain text"), "plain text");
    }

    #[test]
    fn test_strip_ansi_complex() {
        let input = "\x1b[32mGreen\x1b[0m normal \x1b[31mRed\x1b[0m";
        assert_eq!(strip_ansi(input), "Green normal Red");
    }

    #[test]
    fn test_execute_command_success() {
        let result = execute_command("echo", &["test"]);
        assert!(result.is_ok());
        let (stdout, _, code) = result.unwrap();
        assert_eq!(code, 0);
        assert!(stdout.contains("test"));
    }

    #[test]
    fn test_execute_command_failure() {
        let result = execute_command("nonexistent_command_xyz_12345", &[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_format_tokens_millions() {
        assert_eq!(format_tokens(1_234_567), "1.2M");
        assert_eq!(format_tokens(12_345_678), "12.3M");
    }

    #[test]
    fn test_format_tokens_thousands() {
        assert_eq!(format_tokens(59_234), "59.2K");
        assert_eq!(format_tokens(1_000), "1.0K");
    }

    #[test]
    fn test_format_tokens_small() {
        assert_eq!(format_tokens(694), "694");
        assert_eq!(format_tokens(0), "0");
    }

    #[test]
    fn test_format_usd_large() {
        assert_eq!(format_usd(1234.567), "$1234.57");
        assert_eq!(format_usd(1000.0), "$1000.00");
    }

    #[test]
    fn test_format_usd_medium() {
        assert_eq!(format_usd(12.345), "$12.35");
        assert_eq!(format_usd(0.99), "$0.99");
    }

    #[test]
    fn test_join_with_overflow_appends_localized_suffix() {
        let items = vec!["a".to_string(), "b".to_string()];
        assert_eq!(
            join_with_overflow(&items, 5, 2, "实例"),
            "a\nb\n... +3 个实例"
        );
    }

    #[test]
    fn test_join_with_overflow_without_overflow() {
        let items = vec!["a".to_string(), "b".to_string()];
        assert_eq!(join_with_overflow(&items, 2, 2, "实例"), "a\nb");
    }

    #[test]
    fn test_format_usd_small() {
        assert_eq!(format_usd(0.0096), "$0.0096");
        assert_eq!(format_usd(0.0001), "$0.0001");
    }

    #[test]
    fn test_format_usd_edge() {
        assert_eq!(format_usd(0.01), "$0.01");
        assert_eq!(format_usd(0.009), "$0.0090");
    }

    #[test]
    fn test_ok_confirmation_with_detail() {
        assert_eq!(ok_confirmation("合并", "#42"), "已合并 #42");
        assert_eq!(
            ok_confirmation("创建", "PR #5 https://github.com/foo/bar/pull/5"),
            "已创建 PR #5 https://github.com/foo/bar/pull/5"
        );
    }

    #[test]
    fn test_ok_confirmation_no_detail() {
        assert_eq!(ok_confirmation("评论", ""), "已评论");
    }

    #[test]
    fn test_format_cpt_normal() {
        assert_eq!(format_cpt(0.000003), "$3.00/MTok");
        assert_eq!(format_cpt(0.0000038), "$3.80/MTok");
        assert_eq!(format_cpt(0.00000386), "$3.86/MTok");
    }

    #[test]
    fn test_format_cpt_edge_cases() {
        assert_eq!(format_cpt(0.0), "$0.00/MTok"); // zero
        assert_eq!(format_cpt(-0.000001), "$0.00/MTok"); // negative
        assert_eq!(format_cpt(f64::INFINITY), "$0.00/MTok"); // infinite
        assert_eq!(format_cpt(f64::NAN), "$0.00/MTok"); // NaN
    }

    #[test]
    fn test_detect_package_manager_default() {
        // 在测试环境（rtk 仓库）中通常没有 JS lockfile，
        // 因此默认应落到 `npm`
        let pm = detect_package_manager();
        assert!(["pnpm", "yarn", "npm"].contains(&pm));
    }

    #[test]
    fn test_truncate_multibyte_thai() {
        // 泰文字符通常占 3 字节
        let thai = "สวัสดีครับ";
        let result = truncate(thai, 5);
        // 不应 panic，且结果必须是合法 UTF-8
        assert!(result.len() <= thai.len());
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_multibyte_emoji() {
        let emoji = "🎉🎊🎈🎁🎂🎄🎃🎆🎇✨";
        let result = truncate(emoji, 5);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_truncate_multibyte_cjk() {
        let cjk = "你好世界测试字符串";
        let result = truncate(cjk, 6);
        assert!(result.ends_with("..."));
    }

    // ===== `resolve_binary` 测试（issue #212） =====

    #[test]
    fn test_resolve_binary_finds_known_command() {
        // 在任何 Rust 开发环境中，`cargo` 都应存在于 PATH 中
        let result = resolve_binary("cargo");
        assert!(
            result.is_ok(),
            "resolve_binary('cargo') 应成功，实际得到：{:?}",
            result.err()
        );
    }

    #[test]
    fn test_resolve_binary_returns_absolute_path() {
        let path = resolve_binary("cargo").expect("应能解析到 cargo");
        assert!(
            path.is_absolute(),
            "resolve_binary 应返回绝对路径，实际得到：{path:?}"
        );
    }

    #[test]
    fn test_resolve_binary_fails_for_unknown() {
        let result = resolve_binary("nonexistent_binary_xyz_99999");
        assert!(result.is_err(), "不存在的二进制应导致 resolve_binary 失败");
    }

    #[test]
    fn test_resolve_binary_path_contains_binary_name() {
        let path = resolve_binary("cargo").expect("应能解析到 cargo");
        let filename = path.file_name().expect("应包含文件名").to_string_lossy();
        // Windows 上可能是 `cargo.exe`，Unix 上通常就是 `cargo`
        assert!(
            filename.starts_with("cargo"),
            "解析后的文件名应以 'cargo' 开头，实际得到：{filename}"
        );
    }

    // ===== `resolved_command` 测试（issue #212） =====

    #[test]
    fn test_resolved_command_executes_known_command() {
        let output = resolved_command("cargo")
            .arg("--version")
            .output()
            .expect("resolved_command('cargo') 应可执行");
        assert!(
            output.status.success(),
            "通过 resolved_command 执行 cargo --version 应成功"
        );
    }

    // ===== `tool_exists` 测试（issue #212） =====

    #[test]
    fn test_tool_exists_finds_cargo() {
        assert!(tool_exists("cargo"), "tool_exists('cargo') 应返回 true");
    }

    #[test]
    fn test_tool_exists_rejects_unknown() {
        assert!(
            !tool_exists("nonexistent_binary_xyz_99999"),
            "对于不存在的二进制，tool_exists 应返回 false"
        );
    }

    #[test]
    fn test_tool_exists_finds_git() {
        assert!(tool_exists("git"), "tool_exists('git') 应返回 true");
    }

    // ===== Windows 专属 PATHEXT 解析测试（issue #212） =====

    #[cfg(target_os = "windows")]
    mod windows_tests {
        use super::super::*;
        use std::fs;

        /// 创建临时 `.cmd` 包装器，模拟 Node.js 工具安装
        fn create_temp_cmd_wrapper(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
            let cmd_path = dir.join(format!("{}.cmd", name));
            fs::write(&cmd_path, "@echo off\r\necho fake-tool-output\r\n")
                .expect("创建 .cmd 包装器失败");
            cmd_path
        }

        /// 构造一个包含临时目录的 PATH 字符串
        fn path_with_dir(dir: &std::path::Path) -> std::ffi::OsString {
            let original = std::env::var_os("PATH").unwrap_or_default();
            let mut new_path = std::ffi::OsString::from(dir.as_os_str());
            new_path.push(";");
            new_path.push(&original);
            new_path
        }

        #[test]
        fn test_resolve_binary_finds_cmd_wrapper() {
            let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
            create_temp_cmd_wrapper(temp_dir.path(), "fake-tool-test");

            // 使用 `which::which_in`，避免修改全局 PATH（线程安全）
            let search_path = path_with_dir(temp_dir.path());
            let result = which::which_in(
                "fake-tool-test",
                Some(search_path),
                std::env::current_dir().unwrap(),
            );

            assert!(
                result.is_ok(),
                "在 Windows 上，which_in 应能找到 .cmd 包装器，实际得到：{:?}",
                result.err()
            );

            let path = result.unwrap();
            let ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            assert!(
                ext == "cmd" || ext == "bat",
                "解析后的路径应带有 .cmd/.bat 后缀，实际得到：{:?}",
                path
            );
        }

        #[test]
        fn test_resolve_binary_finds_bat_wrapper() {
            let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
            let bat_path = temp_dir.path().join("fake-bat-tool.bat");
            fs::write(&bat_path, "@echo off\r\necho bat-output\r\n").expect("创建 .bat 包装器失败");

            let search_path = path_with_dir(temp_dir.path());
            let result = which::which_in(
                "fake-bat-tool",
                Some(search_path),
                std::env::current_dir().unwrap(),
            );

            assert!(
                result.is_ok(),
                "在 Windows 上，which_in 应能找到 .bat 包装器，实际得到：{:?}",
                result.err()
            );
        }

        #[test]
        fn test_resolved_command_executes_cmd_wrapper() {
            let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
            create_temp_cmd_wrapper(temp_dir.path(), "fake-exec-test");

            // 先解析完整路径，再直接执行（不修改 PATH）
            let search_path = path_with_dir(temp_dir.path());
            let resolved = which::which_in(
                "fake-exec-test",
                Some(search_path),
                std::env::current_dir().unwrap(),
            )
            .expect("应能解析 fake-exec-test");

            let output = Command::new(&resolved).output();

            assert!(
                output.is_ok(),
                "在 Windows 上，使用已解析路径的 Command 应能执行 .cmd 包装器"
            );
            let output = output.unwrap();
            let stdout = crate::utils::decode_output(&output.stdout);
            assert!(
                stdout.contains("fake-tool-output"),
                "应从 .cmd 包装器获得输出，实际得到：{}",
                stdout
            );
        }

        #[test]
        fn test_resolved_command_fallback_on_unknown_binary() {
            // 当 `resolve_binary` 失败时，`resolved_command` 应回退到
            // `Command::new(name)`，而不是 panic。在 Windows 上还会
            // 额外向 stderr 打出警告。
            let mut cmd = resolved_command("nonexistent_binary_xyz_99999");
            // `Command` 应能成功创建（不能 panic）。
            // 运行它失败是预期行为；这里只验证回退路径仍能产出可用的 `Command`。
            let result = cmd.output();
            assert!(
                result.is_err() || !result.unwrap().status.success(),
                "不存在的二进制应执行失败，但 resolved_command 不得 panic"
            );
        }

        #[test]
        fn test_tool_exists_finds_cmd_wrapper() {
            let temp_dir = tempfile::tempdir().expect("创建临时目录失败");
            create_temp_cmd_wrapper(temp_dir.path(), "fake-exists-test");

            let search_path = path_with_dir(temp_dir.path());
            let result = which::which_in(
                "fake-exists-test",
                Some(search_path),
                std::env::current_dir().unwrap(),
            );

            assert!(
                result.is_ok(),
                "在 Windows 上，which_in 应能找到 .cmd 包装器"
            );
        }
    }
}

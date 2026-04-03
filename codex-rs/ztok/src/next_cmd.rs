use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::strip_ansi;
use crate::utils::tool_exists;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use regex::Regex;

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 优先直接尝试 `next`，找不到时再回退到 `npx`
    let next_exists = tool_exists("next");

    let mut cmd = if next_exists {
        resolved_command("next")
    } else {
        let mut c = resolved_command("npx");
        c.arg("next");
        c
    };

    cmd.arg("build");

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        let tool = if next_exists { "next" } else { "npx next" };
        eprintln!("运行：{tool} build");
    }

    let output = cmd
        .output()
        .context("运行 next build 失败（可尝试：npm install -g next）")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = filter_next_build(&raw);

    println!("{filtered}");

    timer.track("next build", "ztok next build", &raw, &filtered);

    // 保留退出码，兼容 CI/CD
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

/// 过滤 Next.js 构建输出，提取路由、Bundle 和告警信息
fn filter_next_build(output: &str) -> String {
    lazy_static::lazy_static! {
        // 路由行模式：○ /dashboard    1.2 kB  132 kB
        static ref ROUTE_PATTERN: Regex = crate::utils::compile_regex(
            r"^[○●◐λ✓]\s+(/[^\s]*)\s+(\d+(?:\.\d+)?)\s*(kB|B)"
        );

        // 用于匹配 Bundle 体积的模式
        static ref BUNDLE_PATTERN: Regex = crate::utils::compile_regex(
            r"^[○●◐λ✓]\s+([\w/\-\.]+)\s+(\d+(?:\.\d+)?)\s*(kB|B)\s+(\d+(?:\.\d+)?)\s*(kB|B)"
        );
    }

    let mut routes_static = 0;
    let mut routes_dynamic = 0;
    let mut routes_total = 0;
    let mut bundles: Vec<(String, f64, Option<f64>)> = Vec::new();
    let mut warnings = 0;
    let mut errors = 0;
    let mut build_time = String::new();

    // 去除 ANSI 颜色码
    let clean_output = strip_ansi(output);

    for line in clean_output.lines() {
        // 按符号统计路由类型
        if line.starts_with("○") {
            routes_static += 1;
            routes_total += 1;
        } else if line.starts_with("●") || line.starts_with("◐") {
            routes_dynamic += 1;
            routes_total += 1;
        } else if line.starts_with("λ") {
            routes_total += 1;
        }

        // 提取 Bundle 信息（路由 + 大小 + 总大小）
        if let Some(caps) = BUNDLE_PATTERN.captures(line) {
            let route = caps[1].to_string();
            let size: f64 = caps[2].parse().unwrap_or(0.0);
            let total: f64 = caps[4].parse().unwrap_or(0.0);

            // 若两个大小都存在，则计算增幅百分比
            let pct_change = if total > 0.0 {
                Some(((total - size) / size) * 100.0)
            } else {
                None
            };

            bundles.push((route, total, pct_change));
        }

        // 统计警告和错误
        if line.to_lowercase().contains("warning") {
            warnings += 1;
        }
        if line.to_lowercase().contains("error") && !line.contains("0 error") {
            errors += 1;
        }

        // 提取构建耗时
        if (line.contains("Compiled") || line.contains("in"))
            && let Some(time_match) = extract_time(line)
        {
            build_time = time_match;
        }
    }

    // 检测是否跳过了构建（已构建 / 使用缓存）
    let already_built = clean_output.contains("already optimized")
        || clean_output.contains("Cache")
        || (routes_total == 0 && clean_output.contains("Ready"));

    // 构建过滤后的输出
    let mut result = String::new();
    result.push_str("⚡ Next.js 构建\n");
    result.push_str("═══════════════════════════════════════\n");

    if already_built && routes_total == 0 {
        result.push_str("✓ 已构建（使用缓存）\n\n");
    } else if routes_total > 0 {
        result.push_str(&format!(
            "✓ {routes_total} 条路由（{routes_static} 静态，{routes_dynamic} 动态）\n\n"
        ));
    }

    if !bundles.is_empty() {
        result.push_str("Bundles:\n");

        // 按体积降序排序并展示前 10 条
        bundles.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (route, size, pct_change) in bundles.iter().take(10) {
            let warning_marker = if let Some(pct) = pct_change {
                if *pct > 10.0 {
                    format!(" ⚠️ (+{pct:.0}%)")
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            result.push_str(&format!(
                "  {:<30} {:>6.0} kB{}\n",
                truncate(route, /*max_len*/ 30),
                size,
                warning_marker
            ));
        }

        if bundles.len() > 10 {
            result.push_str(&format!("\n  ... +{} 条路由\n", bundles.len() - 10));
        }

        result.push('\n');
    }

    // 输出构建耗时和状态
    if !build_time.is_empty() {
        result.push_str(&format!("耗时：{build_time} | "));
    }

    result.push_str(&format!("错误：{errors} | 警告：{warnings}\n"));

    result.trim().to_string()
}

/// 从构建输出中提取耗时（例如 `"Compiled in 34.2s"`）
fn extract_time(line: &str) -> Option<String> {
    lazy_static::lazy_static! {
        static ref TIME_RE: Regex = crate::utils::compile_regex(r"(\d+(?:\.\d+)?)\s*(s|ms)");
    }

    TIME_RE
        .captures(line)
        .map(|caps| format!("{}{}", &caps[1], &caps[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_next_build() {
        let output = r#"
   ▲ Next.js 15.2.0

   Creating an optimized production build ...
✓ Compiled successfully
✓ Linting and checking validity of types
✓ Collecting page data
○ /                            1.2 kB        132 kB
● /dashboard                   2.5 kB        156 kB
○ /api/auth                    0.5 kB         89 kB

Route (app)                    Size     First Load JS
┌ ○ /                          1.2 kB        132 kB
├ ● /dashboard                 2.5 kB        156 kB
└ ○ /api/auth                  0.5 kB         89 kB

○  (Static)  prerendered as static content
●  (SSG)     prerendered as static HTML
λ  (Server)  server-side renders at runtime

✓ Built in 34.2s
"#;
        let result = filter_next_build(output);
        assert!(result.contains("⚡ Next.js 构建"));
        assert!(result.contains("路由"));
        assert!(!result.contains("Creating an optimized")); // 应过滤冗长日志
    }

    #[test]
    fn test_extract_time() {
        assert_eq!(extract_time("Built in 34.2s"), Some("34.2s".to_string()));
        assert_eq!(
            extract_time("Compiled in 1250ms"),
            Some("1250ms".to_string())
        );
        assert_eq!(extract_time("No time here"), None);
    }
}

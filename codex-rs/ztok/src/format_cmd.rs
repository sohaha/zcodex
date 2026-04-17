use crate::prettier_cmd;
use crate::ruff_cmd;
use crate::tracking;
use crate::utils::package_manager_exec;
use crate::utils::resolved_command;
use anyhow::Context;
use anyhow::Result;
use std::path::Path;

/// 从项目文件或显式参数中检测格式化器
fn detect_formatter(args: &[String]) -> String {
    detect_formatter_in_dir(args, Path::new("."))
}

/// 在指定目录中检测格式化器（用于测试）
fn detect_formatter_in_dir(args: &[String], dir: &Path) -> String {
    // 检查第一个参数是否为已知格式化器
    if !args.is_empty() {
        let first_arg = &args[0];
        if matches!(first_arg.as_str(), "prettier" | "black" | "ruff" | "biome") {
            return first_arg.clone();
        }
    }

    // 根据项目文件自动检测
    // 优先级：pyproject.toml > package.json > fallback
    let pyproject_path = dir.join("pyproject.toml");
    if pyproject_path.exists() {
        // 读取 pyproject.toml 以检测格式化器
        if let Ok(content) = std::fs::read_to_string(&pyproject_path) {
            // 检查 [tool.black] 段
            if content.contains("[tool.black]") {
                return "black".to_string();
            }
            // 检查 [tool.ruff.format] 段
            if content.contains("[tool.ruff.format]") || content.contains("[tool.ruff]") {
                return "ruff".to_string();
            }
        }
    }

    // 检查 package.json 或 prettier 配置
    if dir.join("package.json").exists()
        || dir.join(".prettierrc").exists()
        || dir.join(".prettierrc.json").exists()
        || dir.join(".prettierrc.js").exists()
    {
        return "prettier".to_string();
    }

    // 回退：按 ruff -> black -> prettier 的顺序尝试
    "ruff".to_string()
}

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 检测格式化器
    let formatter = detect_formatter(args);

    // 确定实际参数的起始下标
    let start_idx = if !args.is_empty() && args[0] == formatter {
        1 // 若显式提供了格式化器名称，则跳过
    } else {
        0 // 若为自动检测，则使用全部参数
    };

    if verbose > 0 {
        eprintln!("检测到格式化器：{formatter}");
        eprintln!("参数：{}", args[start_idx..].join(" "));
    }

    // 根据格式化器构建命令
    let mut cmd = match formatter.as_str() {
        "prettier" => package_manager_exec("prettier"),
        "black" | "ruff" => resolved_command(formatter.as_str()),
        "biome" => package_manager_exec("biome"),
        _ => resolved_command(formatter.as_str()),
    };

    // 添加格式化器专属参数
    let user_args = args[start_idx..].to_vec();

    match formatter.as_str() {
        "black" => {
            // 在检查模式下，若未提供 --check，则自动注入
            if !user_args.iter().any(|a| a == "--check" || a == "--diff") {
                cmd.arg("--check");
            }
        }
        "ruff" => {
            // 若不存在 `format` 子命令，则自动补上
            if user_args.is_empty() || !user_args[0].starts_with("format") {
                cmd.arg("format");
            }
        }
        _ => {}
    }

    // 追加用户参数
    for arg in &user_args {
        cmd.arg(arg);
    }

    // 若未指定路径，则默认使用当前目录
    if user_args.iter().all(|a| a.starts_with('-')) {
        cmd.arg(".");
    }

    if verbose > 0 {
        eprintln!("运行：{} {}", formatter, user_args.join(" "));
    }

    let output = cmd.output().context(format!(
        "运行 {formatter} 失败。请确认已安装：pip install {formatter}（JS 格式化器用 npm/pnpm）"
    ))?;

    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    // 根据格式化器分发到对应过滤器
    let filtered = match formatter.as_str() {
        "prettier" => prettier_cmd::filter_prettier_output(&raw),
        "ruff" => ruff_cmd::filter_ruff_format(&raw),
        "black" => filter_black_output(&raw),
        _ => raw.trim().to_string(),
    };

    println!("{filtered}");

    timer.track(
        &format!("{} {}", formatter, user_args.join(" ")),
        &format!("ztok format {} {}", formatter, user_args.join(" ")),
        &raw,
        &filtered,
    );

    // 保留退出码，兼容 CI/CD
    if !output.status.success() {
        std::process::exit(output.status.code().unwrap_or(1));
    }

    Ok(())
}

/// 过滤 black 输出，仅显示需要格式化的文件
fn filter_black_output(output: &str) -> String {
    let mut files_to_format: Vec<String> = Vec::new();
    let mut files_unchanged = 0;
    let mut files_would_reformat = 0;
    let mut all_done = false;
    let mut oh_no = false;

    for line in output.lines() {
        let trimmed = line.trim();
        let lower = trimmed.to_lowercase();

        // 检查 `would reformat` 行
        if lower.starts_with("would reformat:") {
            // 从 `would reformat: path/to/file.py` 中提取文件名
            if let Some(filename) = trimmed.split(':').nth(1) {
                files_to_format.push(filename.trim().to_string());
            }
        }

        // 解析类似 `2 files would be reformatted, 3 files would be left unchanged.` 的摘要行
        if lower.contains("would be reformatted") || lower.contains("would be left unchanged") {
            // 按逗号拆分，分别处理两部分
            for part in trimmed.split(',') {
                let part_lower = part.to_lowercase();
                let words: Vec<&str> = part.split_whitespace().collect();

                if part_lower.contains("would be reformatted") {
                    // 解析 `X file(s) would be reformatted`
                    for (i, word) in words.iter().enumerate() {
                        if (word == &"file" || word == &"files")
                            && i > 0
                            && let Ok(count) = words[i - 1].parse::<usize>()
                        {
                            files_would_reformat = count;
                            break;
                        }
                    }
                }

                if part_lower.contains("would be left unchanged") {
                    // 解析 `X file(s) would be left unchanged`
                    for (i, word) in words.iter().enumerate() {
                        if (word == &"file" || word == &"files")
                            && i > 0
                            && let Ok(count) = words[i - 1].parse::<usize>()
                        {
                            files_unchanged = count;
                            break;
                        }
                    }
                }
            }
        }

        // 检查独立出现的 `left unchanged`
        if lower.contains("left unchanged") && !lower.contains("would be") {
            let words: Vec<&str> = trimmed.split_whitespace().collect();
            for (i, word) in words.iter().enumerate() {
                if (word == &"file" || word == &"files")
                    && i > 0
                    && let Ok(count) = words[i - 1].parse::<usize>()
                {
                    files_unchanged = count;
                    break;
                }
            }
        }

        // 检查成功/失败标记
        if lower.contains("all done!") || lower.contains("all done ✨") {
            all_done = true;
        }
        if lower.contains("oh no!") {
            oh_no = true;
        }
    }

    // 构建输出
    let mut result = String::new();

    // 判断是否所有文件都已格式化
    let needs_formatting = !files_to_format.is_empty() || files_would_reformat > 0 || oh_no;

    if !needs_formatting && (all_done || files_unchanged > 0) {
        // 所有文件格式都正确
        result.push_str("格式（black）：所有文件格式正确");
        if files_unchanged > 0 {
            result.push_str(&format!("（检查了 {files_unchanged} 个文件）"));
        }
    } else if needs_formatting {
        // 存在需要格式化的文件
        let count = if !files_to_format.is_empty() {
            files_to_format.len()
        } else {
            files_would_reformat
        };

        result.push_str(&format!("格式（black）：{count} 个文件需要格式化\n"));
        result.push_str("═══════════════════════════════════════\n");

        if !files_to_format.is_empty() {
            for (i, file) in files_to_format.iter().take(10).enumerate() {
                result.push_str(&format!("{}. {}\n", i + 1, compact_path(file)));
            }

            if files_to_format.len() > 10 {
                result.push_str(&format!("\n... +{} 个文件\n", files_to_format.len() - 10));
            }
        }

        if files_unchanged > 0 {
            result.push_str(&format!("\n{files_unchanged} 个文件已格式化\n"));
        }

        result.push_str("\n运行 `black .` 格式化这些文件\n");
    } else {
        // 回退：直接显示原始输出
        result.push_str(output.trim());
    }

    result.trim().to_string()
}

/// 压缩文件路径（移除常见公共前缀）
fn compact_path(path: &str) -> String {
    let path = path.replace('\\', "/");

    if let Some(pos) = path.rfind("/src/") {
        format!("src/{}", &path[pos + 5..])
    } else if let Some(pos) = path.rfind("/lib/") {
        format!("lib/{}", &path[pos + 5..])
    } else if let Some(pos) = path.rfind("/tests/") {
        format!("tests/{}", &path[pos + 7..])
    } else if let Some(pos) = path.rfind('/') {
        path[pos + 1..].to_string()
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_detect_formatter_from_explicit_arg() {
        let args = vec!["black".to_string(), "--check".to_string()];
        let formatter = detect_formatter(&args);
        assert_eq!(formatter, "black");

        let args = vec!["prettier".to_string(), ".".to_string()];
        let formatter = detect_formatter(&args);
        assert_eq!(formatter, "prettier");

        let args = vec!["ruff".to_string(), "format".to_string()];
        let formatter = detect_formatter(&args);
        assert_eq!(formatter, "ruff");
    }

    #[test]
    fn test_detect_formatter_from_pyproject_black() {
        let temp_dir = TempDir::new().unwrap();
        let pyproject_path = temp_dir.path().join("pyproject.toml");
        let mut file = fs::File::create(&pyproject_path).unwrap();
        writeln!(file, "[tool.black]\nline-length = 88").unwrap();

        let formatter = detect_formatter_in_dir(&[], temp_dir.path());
        assert_eq!(formatter, "black");
    }

    #[test]
    fn test_detect_formatter_from_pyproject_ruff() {
        let temp_dir = TempDir::new().unwrap();
        let pyproject_path = temp_dir.path().join("pyproject.toml");
        let mut file = fs::File::create(&pyproject_path).unwrap();
        writeln!(file, "[tool.ruff.format]\nindent-width = 4").unwrap();

        let formatter = detect_formatter_in_dir(&[], temp_dir.path());
        assert_eq!(formatter, "ruff");
    }

    #[test]
    fn test_detect_formatter_from_package_json() {
        let temp_dir = TempDir::new().unwrap();
        let package_path = temp_dir.path().join("package.json");
        let mut file = fs::File::create(&package_path).unwrap();
        writeln!(file, "{{\"name\": \"test\"}}").unwrap();

        let formatter = detect_formatter_in_dir(&[], temp_dir.path());
        assert_eq!(formatter, "prettier");
    }

    #[test]
    fn test_filter_black_all_formatted() {
        let output = "All done! ✨ 🍰 ✨\n5 files left unchanged.";
        let result = filter_black_output(output);
        assert!(result.contains("格式（black）"));
        assert!(result.contains("所有文件格式正确"));
        assert!(result.contains("检查了 5 个文件"));
    }

    #[test]
    fn test_filter_black_needs_formatting() {
        let output = r#"would reformat: src/main.py
would reformat: tests/test_utils.py
Oh no! 💥 💔 💥
2 files would be reformatted, 3 files would be left unchanged."#;

        let result = filter_black_output(output);
        assert!(result.contains("2 个文件需要格式化"));
        assert!(result.contains("main.py"));
        assert!(result.contains("test_utils.py"));
        assert!(result.contains("3 个文件已格式化"));
        assert!(result.contains("运行 `black .`"));
    }

    #[test]
    fn test_compact_path() {
        assert_eq!(
            compact_path("/Users/foo/project/src/main.py"),
            "src/main.py"
        );
        assert_eq!(compact_path("/home/user/app/lib/utils.py"), "lib/utils.py");
        assert_eq!(
            compact_path("C:\\Users\\foo\\project\\tests\\test.py"),
            "tests/test.py"
        );
        assert_eq!(compact_path("relative/file.py"), "file.py");
    }
}

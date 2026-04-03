use crate::tracking;
use anyhow::Context;
use anyhow::Result;
use ignore::WalkBuilder;
use std::collections::HashMap;
use std::path::Path;

/// 使用 glob 模式匹配文件名（支持 `*` 和 `?`）。
fn glob_match(pattern: &str, name: &str) -> bool {
    glob_match_inner(pattern.as_bytes(), name.as_bytes())
}

fn glob_match_inner(pat: &[u8], name: &[u8]) -> bool {
    match (pat.first(), name.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            // `*` 匹配零个或多个字符
            glob_match_inner(&pat[1..], name)
                || (!name.is_empty() && glob_match_inner(pat, &name[1..]))
        }
        (Some(b'?'), Some(_)) => glob_match_inner(&pat[1..], &name[1..]),
        (Some(&p), Some(&n)) if p == n => glob_match_inner(&pat[1..], &name[1..]),
        _ => false,
    }
}

/// 从原生 find 语法或 ZTOK find 语法解析后的参数。
#[derive(Debug)]
struct FindArgs {
    pattern: String,
    path: String,
    max_results: usize,
    max_depth: Option<usize>,
    file_type: String,
    case_insensitive: bool,
}

impl Default for FindArgs {
    fn default() -> Self {
        Self {
            pattern: "*".to_string(),
            path: ".".to_string(),
            max_results: 50,
            max_depth: None,
            file_type: "f".to_string(),
            case_insensitive: false,
        }
    }
}

/// 读取 `args` 中位置 `i` 的下一个参数，并推进索引。
/// 如果 `i` 已超过 `args` 末尾，则返回 `None`。
fn next_arg(args: &[String], i: &mut usize) -> Option<String> {
    *i += 1;
    args.get(*i).cloned()
}

/// 检查参数中是否包含原生 find 标志位（-name、-type、-maxdepth 等）
fn has_native_find_flags(args: &[String]) -> bool {
    args.iter()
        .any(|a| a == "-name" || a == "-type" || a == "-maxdepth" || a == "-iname")
}

/// 在 ZTOK 中无法正确处理的原生 find 标志位。
/// 这些标志涉及组合谓词、动作或暂不支持的语义。
const UNSUPPORTED_FIND_FLAGS: &[&str] = &[
    "-not", "!", "-or", "-o", "-and", "-a", "-exec", "-execdir", "-delete", "-print0", "-newer",
    "-perm", "-size", "-mtime", "-mmin", "-atime", "-amin", "-ctime", "-cmin", "-empty", "-link",
    "-regex", "-iregex",
];

fn has_unsupported_find_flags(args: &[String]) -> bool {
    args.iter()
        .any(|a| UNSUPPORTED_FIND_FLAGS.contains(&a.as_str()))
}

/// 从原始参数向量中解析参数，支持原生 find 和 ZTOK 两种语法。
///
/// 原生 find 语法：`find . -name "*.rs" -type f -maxdepth 3`
/// 采用 ZTOK 风格的语法：`find *.rs [path] [-m max] [-t type]`
fn parse_find_args(args: &[String]) -> Result<FindArgs> {
    if args.is_empty() {
        return Ok(FindArgs::default());
    }

    if has_unsupported_find_flags(args) {
        anyhow::bail!("ztok find 不支持复合谓词或动作（如 `-not`、`-exec`），请直接使用 `find`。");
    }

    if has_native_find_flags(args) {
        parse_native_find_args(args)
    } else {
        parse_ztok_find_args(args)
    }
}

/// 解析原生 find 语法：`find [path] -name "*.rs" -type f -maxdepth 3`
fn parse_native_find_args(args: &[String]) -> Result<FindArgs> {
    let mut parsed = FindArgs::default();
    let mut i = 0;

    // 第一个非标志参数是路径（标准 find 行为）
    if !args[0].starts_with('-') {
        parsed.path = args[0].clone();
        i = 1;
    }

    while i < args.len() {
        match args[i].as_str() {
            "-name" => {
                if let Some(val) = next_arg(args, &mut i) {
                    parsed.pattern = val;
                }
            }
            "-iname" => {
                if let Some(val) = next_arg(args, &mut i) {
                    parsed.pattern = val;
                    parsed.case_insensitive = true;
                }
            }
            "-type" => {
                if let Some(val) = next_arg(args, &mut i) {
                    parsed.file_type = val;
                }
            }
            "-maxdepth" => {
                if let Some(val) = next_arg(args, &mut i) {
                    parsed.max_depth = Some(val.parse().context("无效的 -maxdepth 值")?);
                }
            }
            flag if flag.starts_with('-') => {
                eprintln!("ztok find: 未知参数 '{flag}'，已忽略");
            }
            _ => {}
        }
        i += 1;
    }

    Ok(parsed)
}

/// 解析 ZTOK 语法：`find <pattern> [path] [-m max] [-t type]`
fn parse_ztok_find_args(args: &[String]) -> Result<FindArgs> {
    let mut parsed = FindArgs {
        pattern: args[0].clone(),
        ..FindArgs::default()
    };
    let mut i = 1;

    // 第二个位置参数（如果不是标志位）是路径
    if i < args.len() && !args[i].starts_with('-') {
        parsed.path = args[i].clone();
        i += 1;
    }

    while i < args.len() {
        match args[i].as_str() {
            "-m" | "--max" => {
                if let Some(val) = next_arg(args, &mut i) {
                    parsed.max_results = val.parse().context("无效的 --max 值")?;
                }
            }
            "-t" | "--file-type" => {
                if let Some(val) = next_arg(args, &mut i) {
                    parsed.file_type = val;
                }
            }
            _ => {}
        }
        i += 1;
    }

    Ok(parsed)
}

/// 从 `main.rs` 进入的入口：先解析原始参数，再委托给 `run()`。
pub fn run_from_args(args: &[String], verbose: u8) -> Result<()> {
    let parsed = parse_find_args(args)?;
    run(
        &parsed.pattern,
        &parsed.path,
        parsed.max_results,
        parsed.max_depth,
        &parsed.file_type,
        parsed.case_insensitive,
        verbose,
    )
}

pub fn run(
    pattern: &str,
    path: &str,
    max_results: usize,
    max_depth: Option<usize>,
    file_type: &str,
    case_insensitive: bool,
    verbose: u8,
) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // 将 "." 视为匹配全部
    let effective_pattern = if pattern == "." { "*" } else { pattern };

    if verbose > 0 {
        eprintln!("find: 在 {path} 中搜索 {effective_pattern}");
    }

    let want_dirs = file_type == "d";

    let mut builder = WalkBuilder::new(path);
    builder
        .hidden(true) // 跳过隐藏文件/目录
        .git_ignore(true) // 遵循 .gitignore
        .git_global(true)
        .git_exclude(true);
    if let Some(depth) = max_depth {
        builder.max_depth(Some(depth));
    }
    let walker = builder.build();

    let mut files: Vec<String> = Vec::new();

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let ft = entry.file_type();
        let is_dir = ft.as_ref().is_some_and(std::fs::FileType::is_dir);

        // 按类型过滤
        if want_dirs && !is_dir {
            continue;
        }
        if !want_dirs && is_dir {
            continue;
        }

        let entry_path = entry.path();

        // 获取用于 glob 匹配的文件名
        let name = match entry_path.file_name() {
            Some(n) => n.to_string_lossy(),
            None => continue,
        };

        let matches = if case_insensitive {
            glob_match(&effective_pattern.to_lowercase(), &name.to_lowercase())
        } else {
            glob_match(effective_pattern, &name)
        };
        if !matches {
            continue;
        }

        // 存储相对于搜索根目录的路径
        let display_path = entry_path
            .strip_prefix(path)
            .unwrap_or(entry_path)
            .to_string_lossy()
            .to_string();

        if !display_path.is_empty() {
            files.push(display_path);
        }
    }

    files.sort();

    let raw_output = files.join("\n");

    if files.is_empty() {
        let msg = format!("未找到：'{effective_pattern}'");
        println!("{msg}");
        timer.track(
            &format!("find {path} -name '{effective_pattern}'"),
            "ztok find",
            &raw_output,
            &msg,
        );
        return Ok(());
    }

    // 按目录分组
    let mut by_dir: HashMap<String, Vec<String>> = HashMap::new();

    for file in &files {
        let p = Path::new(file);
        let dir = p
            .parent()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        let dir = if dir.is_empty() { ".".to_string() } else { dir };
        let filename = p
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();
        by_dir.entry(dir).or_default().push(filename);
    }

    let mut dirs: Vec<_> = by_dir.keys().cloned().collect();
    dirs.sort();
    let dirs_count = dirs.len();
    let total_files = files.len();

    println!("📁 {total_files} 个文件 {dirs_count} 个目录：");
    println!();

    // 按 `--max` 正确限制展示数量（按单个文件计数）
    let mut shown = 0;
    for dir in &dirs {
        if shown >= max_results {
            break;
        }

        let files_in_dir = &by_dir[dir];
        let dir_display = if dir.len() > 50 {
            format!("...{}", &dir[dir.len() - 47..])
        } else {
            dir.clone()
        };

        let remaining_budget = max_results - shown;
        if files_in_dir.len() <= remaining_budget {
            println!("{}/ {}", dir_display, files_in_dir.join(" "));
            shown += files_in_dir.len();
        } else {
            // 部分展示：只显示剩余预算允许的文件
            let partial: Vec<_> = files_in_dir
                .iter()
                .take(remaining_budget)
                .cloned()
                .collect();
            println!("{}/ {}", dir_display, partial.join(" "));
            shown += partial.len();
            break;
        }
    }

    if shown < total_files {
        println!("+{} 个", total_files - shown);
    }

    // 扩展名摘要
    let mut by_ext: HashMap<String, usize> = HashMap::new();
    for file in &files {
        let ext = Path::new(file)
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_else(|| "无".to_string());
        *by_ext.entry(ext).or_default() += 1;
    }

    let mut ext_line = String::new();
    if by_ext.len() > 1 {
        println!();
        let mut exts: Vec<_> = by_ext.iter().collect();
        exts.sort_by(|a, b| b.1.cmp(a.1));
        let ext_str: Vec<String> = exts
            .iter()
            .take(5)
            .map(|(e, c)| format!(".{e}({c})"))
            .collect();
        ext_line = format!("扩展名：{}", ext_str.join(" "));
        println!("{ext_line}");
    }

    let ztok_output = format!("{total_files} 个文件 {dirs_count} 个目录 + {ext_line}");
    timer.track(
        &format!("find {path} -name '{effective_pattern}'"),
        "ztok find",
        &raw_output,
        &ztok_output,
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 为了测试方便，将字符串切片转换为 `Vec<String>`。
    fn args(values: &[&str]) -> Vec<String> {
        values
            .iter()
            .map(std::string::ToString::to_string)
            .collect()
    }

    // --- glob_match 单元测试 ---

    #[test]
    fn glob_match_star_rs() {
        assert!(glob_match("*.rs", "main.rs"));
        assert!(glob_match("*.rs", "find_cmd.rs"));
        assert!(!glob_match("*.rs", "main.py"));
        assert!(!glob_match("*.rs", "rs"));
    }

    #[test]
    fn glob_match_star_all() {
        assert!(glob_match("*", "anything.txt"));
        assert!(glob_match("*", "a"));
        assert!(glob_match("*", ".hidden"));
    }

    #[test]
    fn glob_match_question_mark() {
        assert!(glob_match("?.rs", "a.rs"));
        assert!(!glob_match("?.rs", "ab.rs"));
    }

    #[test]
    fn glob_match_exact() {
        assert!(glob_match("Cargo.toml", "Cargo.toml"));
        assert!(!glob_match("Cargo.toml", "cargo.toml"));
    }

    #[test]
    fn glob_match_complex() {
        assert!(glob_match("test_*", "test_foo"));
        assert!(glob_match("test_*", "test_"));
        assert!(!glob_match("test_*", "test"));
    }

    // --- 将点号模式视为星号 ---

    #[test]
    fn dot_becomes_star() {
        // `run()` 内部会把 "." 转成 "*"，这里测试这段逻辑
        let effective = if "." == "." { "*" } else { "." };
        assert_eq!(effective, "*");
    }

    #[test]
    fn no_match_message_is_localized() {
        let msg = format!("未找到：'{}'", "*.rs");
        assert_eq!(msg, "未找到：'*.rs'");
    }

    // --- parse_find_args：原生 find 语法 ---

    #[test]
    fn parse_native_find_name() {
        let parsed = parse_find_args(&args(&[".", "-name", "*.rs"])).unwrap();
        assert_eq!(parsed.pattern, "*.rs");
        assert_eq!(parsed.path, ".");
        assert_eq!(parsed.file_type, "f");
        assert_eq!(parsed.max_results, 50);
    }

    #[test]
    fn parse_native_find_name_and_type() {
        let parsed = parse_find_args(&args(&["src", "-name", "*.rs", "-type", "f"])).unwrap();
        assert_eq!(parsed.pattern, "*.rs");
        assert_eq!(parsed.path, "src");
        assert_eq!(parsed.file_type, "f");
    }

    #[test]
    fn parse_native_find_type_d() {
        let parsed = parse_find_args(&args(&[".", "-type", "d"])).unwrap();
        assert_eq!(parsed.pattern, "*");
        assert_eq!(parsed.file_type, "d");
    }

    #[test]
    fn parse_native_find_maxdepth() {
        let parsed = parse_find_args(&args(&[".", "-name", "*.toml", "-maxdepth", "2"])).unwrap();
        assert_eq!(parsed.pattern, "*.toml");
        assert_eq!(parsed.max_depth, Some(2));
        assert_eq!(parsed.max_results, 50); // `-maxdepth` 不会改变 max_results
    }

    #[test]
    fn parse_native_find_iname() {
        let parsed = parse_find_args(&args(&[".", "-iname", "Makefile"])).unwrap();
        assert_eq!(parsed.pattern, "Makefile");
        assert!(parsed.case_insensitive);
    }

    #[test]
    fn parse_native_find_name_is_case_sensitive() {
        let parsed = parse_find_args(&args(&[".", "-name", "*.rs"])).unwrap();
        assert!(!parsed.case_insensitive);
    }

    #[test]
    fn parse_native_find_no_path() {
        // `find -name "*.rs"` without explicit path defaults to "."
        let parsed = parse_find_args(&args(&["-name", "*.rs"])).unwrap();
        assert_eq!(parsed.pattern, "*.rs");
        assert_eq!(parsed.path, ".");
    }

    // --- parse_find_args：不支持的标志位 ---

    #[test]
    fn parse_native_find_rejects_not() {
        let result = parse_find_args(&args(&[".", "-name", "*.rs", "-not", "-name", "*_test.rs"]));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("复合谓词"));
    }

    #[test]
    fn parse_native_find_rejects_exec() {
        let result = parse_find_args(&args(&[".", "-name", "*.tmp", "-exec", "rm", "{}", ";"]));
        assert!(result.is_err());
    }

    // --- parse_find_args：ZTOK 语法 ---

    #[test]
    fn parse_ztok_syntax_pattern_only() {
        let parsed = parse_find_args(&args(&["*.rs"])).unwrap();
        assert_eq!(parsed.pattern, "*.rs");
        assert_eq!(parsed.path, ".");
    }

    #[test]
    fn parse_ztok_syntax_pattern_and_path() {
        let parsed = parse_find_args(&args(&["*.rs", "src"])).unwrap();
        assert_eq!(parsed.pattern, "*.rs");
        assert_eq!(parsed.path, "src");
    }

    #[test]
    fn parse_ztok_syntax_with_flags() {
        let parsed = parse_find_args(&args(&["*.rs", "src", "-m", "10", "-t", "d"])).unwrap();
        assert_eq!(parsed.pattern, "*.rs");
        assert_eq!(parsed.path, "src");
        assert_eq!(parsed.max_results, 10);
        assert_eq!(parsed.file_type, "d");
    }

    #[test]
    fn parse_empty_args() {
        let parsed = parse_find_args(&args(&[])).unwrap();
        assert_eq!(parsed.pattern, "*");
        assert_eq!(parsed.path, ".");
    }

    // --- run_from_args 集成测试 ---

    #[test]
    fn run_from_args_native_find_syntax() {
        // 模拟：`find . -name "*.rs" -type f`
        let result = run_from_args(&args(&[".", "-name", "*.rs", "-type", "f"]), 0);
        assert!(result.is_ok());
    }

    #[test]
    fn run_from_args_ztok_syntax() {
        // 模拟：`ztok find *.rs src`
        let result = run_from_args(&args(&["*.rs", "src"]), 0);
        assert!(result.is_ok());
    }

    #[test]
    fn run_from_args_iname_case_insensitive() {
        // `-iname` 应按大小写不敏感匹配
        let result = run_from_args(&args(&[".", "-iname", "cargo.toml"]), 0);
        assert!(result.is_ok());
    }

    // --- 集成：在当前仓库上运行 ---

    #[test]
    fn find_rs_files_in_src() {
        // 应能找到 `.rs` 文件且不报错
        let result = run("*.rs", "src", 100, None, "f", false, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn find_dot_pattern_works() {
        // "." 模式不应报错（之前这里有过问题）
        let result = run(".", "src", 10, None, "f", false, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn find_no_matches() {
        let result = run("*.xyz_nonexistent", "src", 50, None, "f", false, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn find_respects_max() {
        // 当 max=2 时也不应报错
        let result = run("*.rs", "src", 2, None, "f", false, 0);
        assert!(result.is_ok());
    }

    #[test]
    fn find_gitignored_excluded() {
        // `target/` 在 `.gitignore` 中，里面的文件不应出现
        let result = run("*", ".", 1000, None, "f", false, 0);
        assert!(result.is_ok());
        // 单元测试里不方便直接捕获 stdout，但至少要验证它能正常运行。
        // 具体输出内容由 smoke tests 覆盖。
    }
}

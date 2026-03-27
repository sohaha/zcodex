//! GitHub CLI（gh）命令输出压缩。
//!
//! 为冗长的 `gh` 命令提供更节省 token 的替代输出。
//! 重点从 JSON 输出中提取关键信息。

use crate::git;
use crate::tracking;
use crate::utils::ok_confirmation;
use crate::utils::resolved_command;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use serde_json::Value;

lazy_static! {
    static ref HTML_COMMENT_RE: Regex = crate::utils::compile_regex(r"(?s)<!--.*?-->");
    static ref BADGE_LINE_RE: Regex =
        crate::utils::compile_regex(r"(?m)^\s*\[!\[[^\]]*\]\([^)]*\)\]\([^)]*\)\s*$");
    static ref IMAGE_ONLY_LINE_RE: Regex =
        crate::utils::compile_regex(r"(?m)^\s*!\[[^\]]*\]\([^)]*\)\s*$");
    static ref HORIZONTAL_RULE_RE: Regex =
        crate::utils::compile_regex(r"(?m)^\s*(?:---+|\*\*\*+|___+)\s*$");
    static ref MULTI_BLANK_RE: Regex = crate::utils::compile_regex(r"\n{3,}");
}

/// 过滤 markdown 正文中的噪声，同时保留有意义的内容。
/// 会移除 HTML 注释、badge 行、纯图片行、分隔线，
/// 并折叠过多空行；代码块内容保持不变。
fn filter_markdown_body(body: &str) -> String {
    if body.is_empty() {
        return String::new();
    }

    // 拆分为代码块与非代码块片段
    let mut result = String::new();
    let mut remaining = body;

    loop {
        // 查找下一个代码块起始标记（``` 或 ~~~）
        let fence_pos = remaining
            .find("```")
            .or_else(|| remaining.find("~~~"))
            .map(|pos| {
                let fence = if remaining[pos..].starts_with("```") {
                    "```"
                } else {
                    "~~~"
                };
                (pos, fence)
            });

        match fence_pos {
            Some((start, fence)) => {
                // 过滤代码块前面的文本
                let before = &remaining[..start];
                result.push_str(&filter_markdown_segment(before));

                // 查找闭合 fence
                let after_open = start + fence.len();
                // 跳过起始 fence 所在行
                let code_start = remaining[after_open..]
                    .find('\n')
                    .map(|p| after_open + p + 1)
                    .unwrap_or(remaining.len());

                let close_pos = remaining[code_start..]
                    .find(fence)
                    .map(|p| code_start + p + fence.len());

                match close_pos {
                    Some(end) => {
                        // 原样保留整个代码块
                        result.push_str(&remaining[start..end]);
                        // 连同闭合 fence 所在行的剩余部分一起保留
                        let after_close = remaining[end..]
                            .find('\n')
                            .map(|p| end + p + 1)
                            .unwrap_or(remaining.len());
                        result.push_str(&remaining[end..after_close]);
                        remaining = &remaining[after_close..];
                    }
                    None => {
                        // 未闭合的代码块：后续内容全部原样保留
                        result.push_str(&remaining[start..]);
                        remaining = "";
                    }
                }
            }
            None => {
                // 没有更多代码块了，过滤剩余文本
                result.push_str(&filter_markdown_segment(remaining));
                break;
            }
        }
    }

    // 最后清理：去掉末尾空白
    result.trim().to_string()
}

/// 过滤不在代码块内的 markdown 片段。
fn filter_markdown_segment(text: &str) -> String {
    let mut s = HTML_COMMENT_RE.replace_all(text, "").to_string();
    s = BADGE_LINE_RE.replace_all(&s, "").to_string();
    s = IMAGE_ONLY_LINE_RE.replace_all(&s, "").to_string();
    s = HORIZONTAL_RULE_RE.replace_all(&s, "").to_string();
    s = MULTI_BLANK_RE.replace_all(&s, "\n\n").to_string();
    s
}

/// 检查参数中是否包含 `--json`（表示用户想要指定 JSON 字段，而不是 RTK 过滤）
fn has_json_flag(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json")
}

/// 从参数中提取位置标识符（PR/issue 编号），
/// 并把它与其余附加参数（如 `-R`、`--repo`）分开返回。
/// 同时处理 `view 123 -R owner/repo` 和 `view -R owner/repo 123` 两种写法。
fn extract_identifier_and_extra_args(args: &[String]) -> Option<(String, Vec<String>)> {
    if args.is_empty() {
        return None;
    }

    // 已知会携带取值的 gh flag：需要连同其值一起跳过
    let flags_with_value = [
        "-R",
        "--repo",
        "-q",
        "--jq",
        "-t",
        "--template",
        "--job",
        "--attempt",
    ];
    let mut identifier = None;
    let mut extra = Vec::new();
    let mut skip_next = false;

    for arg in args {
        if skip_next {
            extra.push(arg.clone());
            skip_next = false;
            continue;
        }
        if flags_with_value.contains(&arg.as_str()) {
            extra.push(arg.clone());
            skip_next = true;
            continue;
        }
        if arg.starts_with('-') {
            extra.push(arg.clone());
            continue;
        }
        // 第一个非 flag 参数就是标识符（编号/URL）
        if identifier.is_none() {
            identifier = Some(arg.clone());
        } else {
            extra.push(arg.clone());
        }
    }

    identifier.map(|id| (id, extra))
}

/// 以节省 token 的方式运行 gh 命令
pub fn run(subcommand: &str, args: &[String], verbose: u8, ultra_compact: bool) -> Result<()> {
    // 用户显式传入 --json 时，期望的是原始 gh JSON，而不是 RTK 过滤结果
    if has_json_flag(args) {
        return run_passthrough("gh", subcommand, args);
    }

    match subcommand {
        "pr" => run_pr(args, verbose, ultra_compact),
        "issue" => run_issue(args, verbose, ultra_compact),
        "run" => run_workflow(args, verbose, ultra_compact),
        "repo" => run_repo(args, verbose, ultra_compact),
        "api" => run_api(args, verbose),
        _ => {
            // 未知子命令，直接透传
            run_passthrough("gh", subcommand, args)
        }
    }
}

fn run_pr(args: &[String], verbose: u8, ultra_compact: bool) -> Result<()> {
    if args.is_empty() {
        return run_passthrough("gh", "pr", args);
    }

    match args[0].as_str() {
        "list" => list_prs(&args[1..], verbose, ultra_compact),
        "view" => view_pr(&args[1..], verbose, ultra_compact),
        "checks" => pr_checks(&args[1..], verbose, ultra_compact),
        "status" => pr_status(verbose, ultra_compact),
        "create" => pr_create(&args[1..], verbose),
        "merge" => pr_merge(&args[1..], verbose),
        "diff" => pr_diff(&args[1..], verbose),
        "comment" => pr_action("评论", args, verbose),
        "edit" => pr_action("编辑", args, verbose),
        _ => run_passthrough("gh", "pr", args),
    }
}

fn list_prs(args: &[String], _verbose: u8, ultra_compact: bool) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args([
        "pr",
        "list",
        "--json",
        "number,title,state,author,updatedAt",
    ]);

    // 透传附加参数
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh pr list 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track("gh pr list", "rtk gh pr list", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let json: Value = serde_json::from_slice(&output.stdout).context("解析 gh pr list 输出失败")?;

    let mut filtered = String::new();

    if let Some(prs) = json.as_array() {
        if ultra_compact {
            filtered.push_str("PR\n");
            println!("PR");
        } else {
            filtered.push_str("📋 PR 列表\n");
            println!("📋 PR 列表");
        }

        for pr in prs.iter().take(20) {
            let number = pr["number"].as_i64().unwrap_or(0);
            let title = pr["title"].as_str().unwrap_or("未知");
            let state = pr["state"].as_str().unwrap_or("未知");
            let author = pr["author"]["login"].as_str().unwrap_or("未知");

            let state_icon = if ultra_compact {
                match state {
                    "OPEN" => "O",
                    "MERGED" => "M",
                    "CLOSED" => "C",
                    _ => "?",
                }
            } else {
                match state {
                    "OPEN" => "🟢",
                    "MERGED" => "🟣",
                    "CLOSED" => "🔴",
                    _ => "⚪",
                }
            };

            let line = format!(
                "  {} #{} {} ({})\n",
                state_icon,
                number,
                truncate(title, /*max_len*/ 60),
                author
            );
            filtered.push_str(&line);
            print!("{line}");
        }

        if prs.len() > 20 {
            let more_line = format!("  ... {} 个（用 gh pr list 查看全部）\n", prs.len() - 20);
            filtered.push_str(&more_line);
            print!("{more_line}");
        }
    }

    timer.track("gh pr list", "rtk gh pr list", &raw, &filtered);
    Ok(())
}

fn should_passthrough_pr_view(extra_args: &[String]) -> bool {
    extra_args
        .iter()
        .any(|a| a == "--json" || a == "--jq" || a == "--web")
}

fn view_pr(args: &[String], _verbose: u8, ultra_compact: bool) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let (pr_number, extra_args) = match extract_identifier_and_extra_args(args) {
        Some(result) => result,
        None => return Err(anyhow::anyhow!("需要 PR 编号")),
    };

    // 如果用户提供了 --jq 或 --web，则直接透传。
    // 注意：--json 已由 run() 通过 has_json_flag 全局处理。
    if should_passthrough_pr_view(&extra_args) {
        return run_passthrough_with_extra("gh", &["pr", "view", &pr_number], &extra_args);
    }

    let mut cmd = resolved_command("gh");
    cmd.args([
        "pr",
        "view",
        &pr_number,
        "--json",
        "number,title,state,author,body,url,mergeable,reviews,statusCheckRollup",
    ]);
    for arg in &extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh pr view 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track(
            &format!("gh pr view {pr_number}"),
            &format!("rtk gh pr view {pr_number}"),
            &stderr,
            &stderr,
        );
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let json: Value = serde_json::from_slice(&output.stdout).context("解析 gh pr view 输出失败")?;

    let mut filtered = String::new();

    // 提取关键信息
    let number = json["number"].as_i64().unwrap_or(0);
    let title = json["title"].as_str().unwrap_or("未知");
    let state = json["state"].as_str().unwrap_or("未知");
    let author = json["author"]["login"].as_str().unwrap_or("未知");
    let url = json["url"].as_str().unwrap_or("");
    let mergeable = json["mergeable"].as_str().unwrap_or("未知");

    let state_icon = if ultra_compact {
        match state {
            "OPEN" => "O",
            "MERGED" => "M",
            "CLOSED" => "C",
            _ => "?",
        }
    } else {
        match state {
            "OPEN" => "🟢",
            "MERGED" => "🟣",
            "CLOSED" => "🔴",
            _ => "⚪",
        }
    };

    let line = format!("{state_icon} PR #{number}：{title}\n");
    filtered.push_str(&line);
    print!("{line}");

    let line = format!("  作者：{author}\n");
    filtered.push_str(&line);
    print!("{line}");

    let mergeable_str = match mergeable {
        "MERGEABLE" => "✓",
        "CONFLICTING" => "✗",
        _ => "?",
    };
    let line = format!("  {state} | {mergeable_str}\n");
    filtered.push_str(&line);
    print!("{line}");

    // 显示评审摘要
    if let Some(reviews) = json["reviews"]["nodes"].as_array() {
        let approved = reviews
            .iter()
            .filter(|r| r["state"].as_str() == Some("APPROVED"))
            .count();
        let changes = reviews
            .iter()
            .filter(|r| r["state"].as_str() == Some("CHANGES_REQUESTED"))
            .count();

        if approved > 0 || changes > 0 {
            let line = format!("  评审：{approved} 通过，{changes} 需修改\n");
            filtered.push_str(&line);
            print!("{line}");
        }
    }

    // 显示检查摘要
    if let Some(checks) = json["statusCheckRollup"].as_array() {
        let total = checks.len();
        let passed = checks
            .iter()
            .filter(|c| {
                c["conclusion"].as_str() == Some("SUCCESS")
                    || c["state"].as_str() == Some("SUCCESS")
            })
            .count();
        let failed = checks
            .iter()
            .filter(|c| {
                c["conclusion"].as_str() == Some("FAILURE")
                    || c["state"].as_str() == Some("FAILURE")
            })
            .count();

        if ultra_compact {
            if failed > 0 {
                let line = format!("  ✗{passed}/{total}  {failed} 失败\n");
                filtered.push_str(&line);
                print!("{line}");
            } else {
                let line = format!("  ✓{passed}/{total}\n");
                filtered.push_str(&line);
                print!("{line}");
            }
        } else {
            let line = format!("  检查：{passed}/{total} 通过\n");
            filtered.push_str(&line);
            print!("{line}");
            if failed > 0 {
                let line = format!("  ⚠️  {failed} 项检查失败\n");
                filtered.push_str(&line);
                print!("{line}");
            }
        }
    }

    let line = format!("  {url}\n");
    filtered.push_str(&line);
    print!("{line}");

    // 显示过滤后的正文
    if let Some(body) = json["body"].as_str()
        && !body.is_empty()
    {
        let body_filtered = filter_markdown_body(body);
        if !body_filtered.is_empty() {
            filtered.push('\n');
            println!();
            for line in body_filtered.lines() {
                let formatted = format!("  {line}\n");
                filtered.push_str(&formatted);
                print!("{formatted}");
            }
        }
    }

    timer.track(
        &format!("gh pr view {pr_number}"),
        &format!("rtk gh pr view {pr_number}"),
        &raw,
        &filtered,
    );
    Ok(())
}

fn pr_checks(args: &[String], _verbose: u8, _ultra_compact: bool) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let (pr_number, extra_args) = match extract_identifier_and_extra_args(args) {
        Some(result) => result,
        None => return Err(anyhow::anyhow!("需要 PR 编号")),
    };

    let mut cmd = resolved_command("gh");
    cmd.args(["pr", "checks", &pr_number]);
    for arg in &extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh pr checks 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track(
            &format!("gh pr checks {pr_number}"),
            &format!("rtk gh pr checks {pr_number}"),
            &stderr,
            &stderr,
        );
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let stdout = crate::utils::decode_output(&output.stdout);

    // 解析并压缩 checks 输出
    let mut passed = 0;
    let mut failed = 0;
    let mut pending = 0;
    let mut failed_checks = Vec::new();

    for line in stdout.lines() {
        if line.contains('✓') || line.contains("pass") {
            passed += 1;
        } else if line.contains('✗') || line.contains("fail") {
            failed += 1;
            failed_checks.push(line.trim().to_string());
        } else if line.contains('*') || line.contains("pending") {
            pending += 1;
        }
    }

    let mut filtered = String::new();

    let line = "🔍 CI 检查摘要：\n";
    filtered.push_str(line);
    print!("{line}");

    let line = format!("  ✅ 通过：{passed}\n");
    filtered.push_str(&line);
    print!("{line}");

    let line = format!("  ❌ 失败：{failed}\n");
    filtered.push_str(&line);
    print!("{line}");

    if pending > 0 {
        let line = format!("  ⏳ 待定：{pending}\n");
        filtered.push_str(&line);
        print!("{line}");
    }

    if !failed_checks.is_empty() {
        let line = "\n  失败的检查：\n";
        filtered.push_str(line);
        print!("{line}");
        for check in failed_checks {
            let line = format!("    {check}\n");
            filtered.push_str(&line);
            print!("{line}");
        }
    }

    timer.track(
        &format!("gh pr checks {pr_number}"),
        &format!("rtk gh pr checks {pr_number}"),
        &raw,
        &filtered,
    );
    Ok(())
}

fn pr_status(_verbose: u8, _ultra_compact: bool) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args([
        "pr",
        "status",
        "--json",
        "currentBranch,createdBy,reviewDecision,statusCheckRollup",
    ]);

    let output = cmd.output().context("运行 gh pr status 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track("gh pr status", "rtk gh pr status", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).context("解析 gh pr status 输出失败")?;

    let mut filtered = String::new();

    if let Some(created_by) = json["createdBy"].as_array() {
        let line = format!("📝 你的 PR（{}）：\n", created_by.len());
        filtered.push_str(&line);
        print!("{line}");
        for pr in created_by.iter().take(5) {
            let number = pr["number"].as_i64().unwrap_or(0);
            let title = pr["title"].as_str().unwrap_or("未知");
            let reviews = pr["reviewDecision"].as_str().unwrap_or("PENDING");
            let line = format!(
                "  #{} {} [{}]\n",
                number,
                truncate(title, /*max_len*/ 50),
                reviews
            );
            filtered.push_str(&line);
            print!("{line}");
        }
    }

    timer.track("gh pr status", "rtk gh pr status", &raw, &filtered);
    Ok(())
}

fn run_issue(args: &[String], verbose: u8, ultra_compact: bool) -> Result<()> {
    if args.is_empty() {
        return run_passthrough("gh", "issue", args);
    }

    match args[0].as_str() {
        "list" => list_issues(&args[1..], verbose, ultra_compact),
        "view" => view_issue(&args[1..], verbose),
        _ => run_passthrough("gh", "issue", args),
    }
}

fn list_issues(args: &[String], _verbose: u8, ultra_compact: bool) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args(["issue", "list", "--json", "number,title,state,author"]);

    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh issue list 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track("gh issue list", "rtk gh issue list", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).context("解析 gh issue list 输出失败")?;

    let mut filtered = String::new();

    if let Some(issues) = json.as_array() {
        if ultra_compact {
            filtered.push_str("议题\n");
            println!("议题");
        } else {
            filtered.push_str("🐛 议题\n");
            println!("🐛 议题");
        }
        for issue in issues.iter().take(20) {
            let number = issue["number"].as_i64().unwrap_or(0);
            let title = issue["title"].as_str().unwrap_or("未知");
            let state = issue["state"].as_str().unwrap_or("未知");

            let icon = if ultra_compact {
                if state == "OPEN" { "O" } else { "C" }
            } else {
                if state == "OPEN" { "🟢" } else { "🔴" }
            };
            let line = format!(
                "  {} #{} {}\n",
                icon,
                number,
                truncate(title, /*max_len*/ 60)
            );
            filtered.push_str(&line);
            print!("{line}");
        }

        if issues.len() > 20 {
            let line = format!("  ... {} 个\n", issues.len() - 20);
            filtered.push_str(&line);
            print!("{line}");
        }
    }

    timer.track("gh issue list", "rtk gh issue list", &raw, &filtered);
    Ok(())
}

fn view_issue(args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let (issue_number, extra_args) = match extract_identifier_and_extra_args(args) {
        Some(result) => result,
        None => return Err(anyhow::anyhow!("需要议题编号")),
    };

    let mut cmd = resolved_command("gh");
    cmd.args([
        "issue",
        "view",
        &issue_number,
        "--json",
        "number,title,state,author,body,url",
    ]);
    for arg in &extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh issue view 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track(
            &format!("gh issue view {issue_number}"),
            &format!("rtk gh issue view {issue_number}"),
            &stderr,
            &stderr,
        );
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).context("解析 gh issue view 输出失败")?;

    let number = json["number"].as_i64().unwrap_or(0);
    let title = json["title"].as_str().unwrap_or("未知");
    let state = json["state"].as_str().unwrap_or("未知");
    let author = json["author"]["login"].as_str().unwrap_or("未知");
    let url = json["url"].as_str().unwrap_or("");

    let icon = if state == "OPEN" { "🟢" } else { "🔴" };

    let mut filtered = String::new();

    let line = format!("{icon} 议题 #{number}：{title}\n");
    filtered.push_str(&line);
    print!("{line}");

    let line = format!("  作者：@{author}\n");
    filtered.push_str(&line);
    print!("{line}");

    let line = format!("  状态：{state}\n");
    filtered.push_str(&line);
    print!("{line}");

    let line = format!("  URL：{url}\n");
    filtered.push_str(&line);
    print!("{line}");

    if let Some(body) = json["body"].as_str()
        && !body.is_empty()
    {
        let body_filtered = filter_markdown_body(body);
        if !body_filtered.is_empty() {
            let line = "\n  描述：\n";
            filtered.push_str(line);
            print!("{line}");
            for line in body_filtered.lines() {
                let formatted = format!("    {line}\n");
                filtered.push_str(&formatted);
                print!("{formatted}");
            }
        }
    }

    timer.track(
        &format!("gh issue view {issue_number}"),
        &format!("rtk gh issue view {issue_number}"),
        &raw,
        &filtered,
    );
    Ok(())
}

fn run_workflow(args: &[String], verbose: u8, ultra_compact: bool) -> Result<()> {
    if args.is_empty() {
        return run_passthrough("gh", "run", args);
    }

    match args[0].as_str() {
        "list" => list_runs(&args[1..], verbose, ultra_compact),
        "view" => view_run(&args[1..], verbose),
        _ => run_passthrough("gh", "run", args),
    }
}

fn list_runs(args: &[String], _verbose: u8, ultra_compact: bool) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args([
        "run",
        "list",
        "--json",
        "databaseId,name,status,conclusion,createdAt",
    ]);
    cmd.arg("--limit").arg("10");

    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh run list 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track("gh run list", "rtk gh run list", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).context("解析 gh run list 输出失败")?;

    let mut filtered = String::new();

    if let Some(runs) = json.as_array() {
        if ultra_compact {
            filtered.push_str("运行\n");
            println!("运行");
        } else {
            filtered.push_str("🏃 工作流运行\n");
            println!("🏃 工作流运行");
        }
        for run in runs {
            let id = run["databaseId"].as_i64().unwrap_or(0);
            let name = run["name"].as_str().unwrap_or("未知");
            let status = run["status"].as_str().unwrap_or("未知");
            let conclusion = run["conclusion"].as_str().unwrap_or("");

            let icon = if ultra_compact {
                match conclusion {
                    "success" => "✓",
                    "failure" => "✗",
                    "cancelled" => "X",
                    _ => {
                        if status == "in_progress" {
                            "~"
                        } else {
                            "?"
                        }
                    }
                }
            } else {
                match conclusion {
                    "success" => "✅",
                    "failure" => "❌",
                    "cancelled" => "🚫",
                    _ => {
                        if status == "in_progress" {
                            "⏳"
                        } else {
                            "⚪"
                        }
                    }
                }
            };

            let line = format!("  {} {} [{}]\n", icon, truncate(name, /*max_len*/ 50), id);
            filtered.push_str(&line);
            print!("{line}");
        }
    }

    timer.track("gh run list", "rtk gh run list", &raw, &filtered);
    Ok(())
}

/// 检查 `run view` 的参数是否应绕过过滤并直接透传。
/// `--log-failed`、`--log`、`--json` 这类 flag 产生的输出若经过过滤，
/// 会被错误裁剪。
fn should_passthrough_run_view(extra_args: &[String]) -> bool {
    extra_args
        .iter()
        .any(|a| a == "--log-failed" || a == "--log" || a == "--json")
}

fn view_run(args: &[String], _verbose: u8) -> Result<()> {
    let (run_id, extra_args) = match extract_identifier_and_extra_args(args) {
        Some(result) => result,
        None => return Err(anyhow::anyhow!("需要 Run ID")),
    };

    // 当用户请求日志或 JSON 时直接透传，否则过滤器会误删内容
    if should_passthrough_run_view(&extra_args) {
        return run_passthrough_with_extra("gh", &["run", "view", &run_id], &extra_args);
    }

    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args(["run", "view", &run_id]);
    for arg in &extra_args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh run view 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track(
            &format!("gh run view {run_id}"),
            &format!("rtk gh run view {run_id}"),
            &stderr,
            &stderr,
        );
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    // 解析输出，仅显示失败项
    let stdout = crate::utils::decode_output(&output.stdout);
    let mut in_jobs = false;

    let mut filtered = String::new();

    let line = format!("🏃 工作流运行 #{run_id}\n");
    filtered.push_str(&line);
    print!("{line}");

    for line in stdout.lines() {
        if line.contains("JOBS") {
            in_jobs = true;
        }

        if in_jobs {
            if line.contains('✓') || line.contains("success") {
                // 紧凑模式下跳过成功的 job
                continue;
            }
            if line.contains('✗') || line.contains("fail") {
                let formatted = format!("  ❌ {}\n", line.trim());
                filtered.push_str(&formatted);
                print!("{formatted}");
            }
        } else if line.contains("Status:") || line.contains("Conclusion:") {
            let formatted_line = line
                .replace("Status:", "状态：")
                .replace("Conclusion:", "结论：");
            let formatted = format!("  {}\n", formatted_line.trim());
            filtered.push_str(&formatted);
            print!("{formatted}");
        }
    }

    timer.track(
        &format!("gh run view {run_id}"),
        &format!("rtk gh run view {run_id}"),
        &raw,
        &filtered,
    );
    Ok(())
}

fn run_repo(args: &[String], _verbose: u8, _ultra_compact: bool) -> Result<()> {
    // 解析子命令（默认为 "view"）
    let (subcommand, rest_args) = if args.is_empty() {
        ("view", args)
    } else {
        (args[0].as_str(), &args[1..])
    };

    if subcommand != "view" {
        return run_passthrough("gh", "repo", args);
    }

    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.arg("repo").arg("view");

    for arg in rest_args {
        cmd.arg(arg);
    }

    cmd.args([
        "--json",
        "name,owner,description,url,stargazerCount,forkCount,isPrivate",
    ]);

    let output = cmd.output().context("运行 gh repo view 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track("gh repo view", "rtk gh repo view", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let json: Value =
        serde_json::from_slice(&output.stdout).context("解析 gh repo view 输出失败")?;

    let name = json["name"].as_str().unwrap_or("未知");
    let owner = json["owner"]["login"].as_str().unwrap_or("未知");
    let description = json["description"].as_str().unwrap_or("");
    let url = json["url"].as_str().unwrap_or("");
    let stars = json["stargazerCount"].as_i64().unwrap_or(0);
    let forks = json["forkCount"].as_i64().unwrap_or(0);
    let private = json["isPrivate"].as_bool().unwrap_or(false);

    let visibility = if private {
        "🔒 私有"
    } else {
        "🌐 公开"
    };

    let mut filtered = String::new();

    let line = format!("📦 {owner}/{name}\n");
    filtered.push_str(&line);
    print!("{line}");

    let line = format!("  {visibility}\n");
    filtered.push_str(&line);
    print!("{line}");

    if !description.is_empty() {
        let line = format!("  {}\n", truncate(description, /*max_len*/ 80));
        filtered.push_str(&line);
        print!("{line}");
    }

    let line = format!("  ⭐ {stars} 星标 | 🔱 {forks} fork\n");
    filtered.push_str(&line);
    print!("{line}");

    let line = format!("  {url}\n");
    filtered.push_str(&line);
    print!("{line}");

    timer.track("gh repo view", "rtk gh repo view", &raw, &filtered);
    Ok(())
}

fn pr_create(args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args(["pr", "create"]);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh pr create 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout).to_string();
    let stderr = crate::utils::decode_output(&output.stderr).to_string();

    if !output.status.success() {
        timer.track("gh pr create", "rtk gh pr create", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    // `gh pr create` 成功时会输出 URL
    let url = stdout.trim();

    // 尝试从 URL 中提取 PR 编号（例如 https://github.com/owner/repo/pull/42）
    let pr_num = url.rsplit('/').next().unwrap_or("");

    let detail = if !pr_num.is_empty() && pr_num.chars().all(|c| c.is_ascii_digit()) {
        format!("#{pr_num} {url}")
    } else {
        url.to_string()
    };

    let filtered = ok_confirmation("创建", &detail);
    println!("{filtered}");

    timer.track("gh pr create", "rtk gh pr create", &stdout, &filtered);
    Ok(())
}

fn pr_merge(args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args(["pr", "merge"]);
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh pr merge 失败")?;
    let stdout = crate::utils::decode_output(&output.stdout).to_string();
    let stderr = crate::utils::decode_output(&output.stderr).to_string();

    if !output.status.success() {
        timer.track("gh pr merge", "rtk gh pr merge", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    // 从参数中提取 PR 编号（第一个非 flag 参数）
    let pr_num = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(std::string::String::as_str)
        .unwrap_or("");

    let detail = if !pr_num.is_empty() {
        format!("#{pr_num}")
    } else {
        String::new()
    };

    let filtered = ok_confirmation("合并", &detail);
    println!("{filtered}");

    // 使用 stdout 或 detail 作为原始输入（gh pr merge 的输出通常很少）
    let raw = if !stdout.trim().is_empty() {
        stdout
    } else {
        detail
    };

    timer.track("gh pr merge", "rtk gh pr merge", &raw, &filtered);
    Ok(())
}

fn pr_diff(args: &[String], _verbose: u8) -> Result<()> {
    // --no-compact: pass full diff through (gh CLI doesn't know this flag, strip it)
    let no_compact = args.iter().any(|a| a == "--no-compact");
    let gh_args: Vec<String> = args
        .iter()
        .filter(|a| *a != "--no-compact")
        .cloned()
        .collect();

    if no_compact {
        return run_passthrough_with_extra("gh", &["pr", "diff"], &gh_args);
    }

    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gh");
    cmd.args(["pr", "diff"]);
    for arg in gh_args.iter() {
        cmd.arg(arg);
    }

    let output = cmd.output().context("运行 gh pr diff 失败")?;
    let raw = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track("gh pr diff", "rtk gh pr diff", &stderr, &stderr);
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    let filtered = if raw.trim().is_empty() {
        let msg = "No diff\n";
        print!("{msg}");
        msg.to_string()
    } else {
        let compacted = git::compact_diff(&raw, /*max_lines*/ 500);
        println!("{compacted}");
        compacted
    };

    timer.track("gh pr diff", "rtk gh pr diff", &raw, &filtered);
    Ok(())
}

/// `comment`/`edit` 共用的通用 PR 动作处理器
fn pr_action(action: &str, args: &[String], _verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();
    let subcmd = &args[0];

    let mut cmd = resolved_command("gh");
    cmd.arg("pr");
    for arg in args {
        cmd.arg(arg);
    }

    let output = cmd.output().context(format!("运行 gh pr {subcmd} 失败"))?;
    let stdout = crate::utils::decode_output(&output.stdout).to_string();

    if !output.status.success() {
        let stderr = crate::utils::decode_output(&output.stderr).to_string();
        timer.track(
            &format!("gh pr {subcmd}"),
            &format!("rtk gh pr {subcmd}"),
            &stderr,
            &stderr,
        );
        eprintln!("{}", stderr.trim());
        std::process::exit(output.status.code().unwrap_or(1));
    }

    // 从参数中提取 PR 编号（跳过 args[0]，因为它是子命令）
    let pr_num = args[1..]
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(|s| format!("#{s}"))
        .unwrap_or_default();

    let filtered = ok_confirmation(action, &pr_num);
    println!("{filtered}");

    // 使用 stdout 或 pr_num 作为原始输入
    let raw = if !stdout.trim().is_empty() {
        stdout
    } else {
        pr_num
    };

    timer.track(
        &format!("gh pr {subcmd}"),
        &format!("rtk gh pr {subcmd}"),
        &raw,
        &filtered,
    );
    Ok(())
}

fn run_api(args: &[String], _verbose: u8) -> Result<()> {
    // `gh api` 是显式的高级命令——用户明确知道自己要什么。
    // 把 JSON 转成 schema 会丢掉所有值，并迫使模型重新抓取数据。
    // 直接透传可以保留完整响应，同时按 0% 节省率记录统计。
    run_passthrough("gh", "api", args)
}

/// 使用基础参数 + 附加参数透传命令，并按“透传”方式记录统计。
fn run_passthrough_with_extra(cmd: &str, base_args: &[&str], extra_args: &[String]) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut command = resolved_command(cmd);
    for arg in base_args {
        command.arg(arg);
    }
    for arg in extra_args {
        command.arg(arg);
    }

    let status = command
        .status()
        .context(format!("运行 {} {} 失败", cmd, base_args.join(" ")))?;

    let full_cmd = format!(
        "{} {} {}",
        cmd,
        base_args.join(" "),
        tracking::args_display(
            &extra_args
                .iter()
                .map(std::convert::Into::into)
                .collect::<Vec<_>>()
        )
    );
    timer.track_passthrough(&full_cmd, &format!("rtk {full_cmd} (passthrough)"));

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

fn run_passthrough(cmd: &str, subcommand: &str, args: &[String]) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut command = resolved_command(cmd);
    command.arg(subcommand);
    for arg in args {
        command.arg(arg);
    }

    let status = command
        .status()
        .context(format!("运行 {cmd} {subcommand} 失败"))?;

    let args_str = tracking::args_display(
        &args
            .iter()
            .map(std::convert::Into::into)
            .collect::<Vec<_>>(),
    );
    timer.track_passthrough(
        &format!("{cmd} {subcommand} {args_str}"),
        &format!("rtk {cmd} {subcommand} {args_str} (passthrough)"),
    );

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(
            truncate("this is a very long string", 15),
            "this is a ve..."
        );
    }

    #[test]
    fn test_truncate_multibyte_utf8() {
        // Emoji：🚀 占 4 字节，但字符数只算 1
        assert_eq!(truncate("🚀🎉🔥abc", 6), "🚀🎉🔥abc"); // 6 chars, fits
        assert_eq!(truncate("🚀🎉🔥abcdef", 8), "🚀🎉🔥ab..."); // 10 chars > 8
        // 边界情况：全部都是多字节字符
        assert_eq!(truncate("🚀🎉🔥🌟🎯", 5), "🚀🎉🔥🌟🎯"); // exact fit
        assert_eq!(truncate("🚀🎉🔥🌟🎯x", 5), "🚀🎉..."); // 6 chars > 5
    }

    #[test]
    fn test_truncate_empty_and_short() {
        assert_eq!(truncate("", 10), "");
        assert_eq!(truncate("ab", 10), "ab");
        assert_eq!(truncate("abc", 3), "abc"); // exact fit
    }

    #[test]
    fn test_ok_confirmation_pr_create() {
        let result = ok_confirmation("创建", "#42 https://github.com/foo/bar/pull/42");
        assert!(result.contains("已创建"));
        assert!(result.contains("#42"));
    }

    #[test]
    fn test_ok_confirmation_pr_merge() {
        let result = ok_confirmation("合并", "#42");
        assert_eq!(result, "已合并 #42");
    }

    #[test]
    fn test_ok_confirmation_pr_comment() {
        let result = ok_confirmation("评论", "#42");
        assert_eq!(result, "已评论 #42");
    }

    #[test]
    fn test_ok_confirmation_pr_edit() {
        let result = ok_confirmation("编辑", "#42");
        assert_eq!(result, "已编辑 #42");
    }

    #[test]
    fn test_has_json_flag_present() {
        assert!(has_json_flag(&[
            "view".into(),
            "--json".into(),
            "number,url".into()
        ]));
    }

    #[test]
    fn test_has_json_flag_absent() {
        assert!(!has_json_flag(&["view".into(), "42".into()]));
    }

    #[test]
    fn test_extract_identifier_simple() {
        let args: Vec<String> = vec!["123".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "123");
        assert!(extra.is_empty());
    }

    #[test]
    fn test_extract_identifier_with_repo_flag_after() {
        // 示例：gh issue view 185 -R rtk-ai/rtk
        let args: Vec<String> = vec!["185".into(), "-R".into(), "rtk-ai/rtk".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "185");
        assert_eq!(extra, vec!["-R", "rtk-ai/rtk"]);
    }

    #[test]
    fn test_extract_identifier_with_repo_flag_before() {
        // 示例：gh issue view -R rtk-ai/rtk 185
        let args: Vec<String> = vec!["-R".into(), "rtk-ai/rtk".into(), "185".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "185");
        assert_eq!(extra, vec!["-R", "rtk-ai/rtk"]);
    }

    #[test]
    fn test_extract_identifier_with_long_repo_flag() {
        let args: Vec<String> = vec!["42".into(), "--repo".into(), "owner/repo".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "42");
        assert_eq!(extra, vec!["--repo", "owner/repo"]);
    }

    #[test]
    fn test_extract_identifier_empty() {
        let args: Vec<String> = vec![];
        assert!(extract_identifier_and_extra_args(&args).is_none());
    }

    #[test]
    fn test_extract_identifier_only_flags() {
        // 没有位置标识符，只有 flags
        let args: Vec<String> = vec!["-R".into(), "rtk-ai/rtk".into()];
        assert!(extract_identifier_and_extra_args(&args).is_none());
    }

    #[test]
    fn test_extract_identifier_with_web_flag() {
        let args: Vec<String> = vec!["123".into(), "--web".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "123");
        assert_eq!(extra, vec!["--web"]);
    }

    #[test]
    fn test_run_view_passthrough_log_failed() {
        assert!(should_passthrough_run_view(&["--log-failed".into()]));
    }

    #[test]
    fn test_run_view_passthrough_log() {
        assert!(should_passthrough_run_view(&["--log".into()]));
    }

    #[test]
    fn test_run_view_passthrough_json() {
        assert!(should_passthrough_run_view(&[
            "--json".into(),
            "jobs".into()
        ]));
    }

    #[test]
    fn test_run_view_no_passthrough_empty() {
        assert!(!should_passthrough_run_view(&[]));
    }

    #[test]
    fn test_run_view_no_passthrough_other_flags() {
        assert!(!should_passthrough_run_view(&["--web".into()]));
    }

    #[test]
    fn test_extract_identifier_with_job_flag_after() {
        // 示例：gh run view 12345 --job 67890
        let args: Vec<String> = vec!["12345".into(), "--job".into(), "67890".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "12345");
        assert_eq!(extra, vec!["--job", "67890"]);
    }

    #[test]
    fn test_extract_identifier_with_job_flag_before() {
        // 示例：gh run view --job 67890 12345
        let args: Vec<String> = vec!["--job".into(), "67890".into(), "12345".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "12345");
        assert_eq!(extra, vec!["--job", "67890"]);
    }

    #[test]
    fn test_extract_identifier_with_job_and_log_failed() {
        // 示例：gh run view --log-failed --job 67890 12345
        let args: Vec<String> = vec![
            "--log-failed".into(),
            "--job".into(),
            "67890".into(),
            "12345".into(),
        ];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "12345");
        assert_eq!(extra, vec!["--log-failed", "--job", "67890"]);
    }

    #[test]
    fn test_extract_identifier_with_attempt_flag() {
        // 示例：gh run view 12345 --attempt 3
        let args: Vec<String> = vec!["12345".into(), "--attempt".into(), "3".into()];
        let (id, extra) = extract_identifier_and_extra_args(&args).unwrap();
        assert_eq!(id, "12345");
        assert_eq!(extra, vec!["--attempt", "3"]);
    }

    // --- should_passthrough_pr_view tests ---

    #[test]
    fn test_should_passthrough_pr_view_json() {
        assert!(should_passthrough_pr_view(&[
            "--json".into(),
            "body,comments".into()
        ]));
    }

    #[test]
    fn test_should_passthrough_pr_view_jq() {
        assert!(should_passthrough_pr_view(&["--jq".into(), ".body".into()]));
    }

    #[test]
    fn test_should_passthrough_pr_view_web() {
        assert!(should_passthrough_pr_view(&["--web".into()]));
    }

    #[test]
    fn test_should_passthrough_pr_view_default() {
        assert!(!should_passthrough_pr_view(&[]));
    }

    #[test]
    fn test_should_passthrough_pr_view_other_flags() {
        assert!(!should_passthrough_pr_view(&["--comments".into()]));
    }

    // --- filter_markdown_body tests ---

    #[test]
    fn test_filter_markdown_body_html_comment_single_line() {
        let input = "Hello\n<!-- this is a comment -->\nWorld";
        let result = filter_markdown_body(input);
        assert!(!result.contains("<!--"));
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
    }

    #[test]
    fn test_filter_markdown_body_html_comment_multiline() {
        let input = "Before\n<!--\nmultiline\ncomment\n-->\nAfter";
        let result = filter_markdown_body(input);
        assert!(!result.contains("<!--"));
        assert!(!result.contains("multiline"));
        assert!(result.contains("Before"));
        assert!(result.contains("After"));
    }

    #[test]
    fn test_filter_markdown_body_badge_lines() {
        let input = "# Title\n[![CI](https://img.shields.io/badge.svg)](https://github.com/actions)\nSome text";
        let result = filter_markdown_body(input);
        assert!(!result.contains("shields.io"));
        assert!(result.contains("# Title"));
        assert!(result.contains("Some text"));
    }

    #[test]
    fn test_filter_markdown_body_image_only_lines() {
        let input = "# Title\n![screenshot](https://example.com/img.png)\nSome text";
        let result = filter_markdown_body(input);
        assert!(!result.contains("![screenshot]"));
        assert!(result.contains("# Title"));
        assert!(result.contains("Some text"));
    }

    #[test]
    fn test_filter_markdown_body_horizontal_rules() {
        let input = "Section 1\n---\nSection 2\n***\nSection 3\n___\nEnd";
        let result = filter_markdown_body(input);
        assert!(!result.contains("---"));
        assert!(!result.contains("***"));
        assert!(!result.contains("___"));
        assert!(result.contains("Section 1"));
        assert!(result.contains("Section 2"));
        assert!(result.contains("Section 3"));
    }

    #[test]
    fn test_filter_markdown_body_blank_lines_collapse() {
        let input = "Line 1\n\n\n\n\nLine 2";
        let result = filter_markdown_body(input);
        // 应折叠为最多一个空行（即连续 2 个换行）
        assert!(!result.contains("\n\n\n"));
        assert!(result.contains("Line 1"));
        assert!(result.contains("Line 2"));
    }

    #[test]
    fn test_filter_markdown_body_code_block_preserved() {
        let input = "Text before\n```python\n<!-- not a comment -->\n![not an image](url)\n---\n```\nText after";
        let result = filter_markdown_body(input);
        // 代码块内部内容应原样保留
        assert!(result.contains("<!-- not a comment -->"));
        assert!(result.contains("![not an image](url)"));
        assert!(result.contains("---"));
        assert!(result.contains("Text before"));
        assert!(result.contains("Text after"));
    }

    #[test]
    fn test_filter_markdown_body_empty() {
        assert_eq!(filter_markdown_body(""), "");
    }

    #[test]
    fn test_filter_markdown_body_meaningful_content_preserved() {
        let input = "## Summary\n- Item 1\n- Item 2\n\n[Link](https://example.com)\n\n| Col1 | Col2 |\n| --- | --- |\n| a | b |";
        let result = filter_markdown_body(input);
        assert!(result.contains("## Summary"));
        assert!(result.contains("- Item 1"));
        assert!(result.contains("- Item 2"));
        assert!(result.contains("[Link](https://example.com)"));
        assert!(result.contains("| Col1 | Col2 |"));
    }

    #[test]
    fn test_filter_markdown_body_token_savings() {
        // 带噪声的真实 PR 正文示例
        let input = r#"<!-- This PR template is auto-generated -->
<!-- Please fill in the following sections -->

## Summary

Added smart markdown filtering for gh issue/pr view commands.

[![CI](https://img.shields.io/github/actions/workflow/status/rtk-ai/rtk/ci.yml)](https://github.com/rtk-ai/rtk/actions)
[![Coverage](https://img.shields.io/codecov/c/github/rtk-ai/rtk)](https://codecov.io/gh/rtk-ai/rtk)

![screenshot](https://user-images.githubusercontent.com/123/screenshot.png)

---

## Changes

- Filter HTML comments
- Filter badge lines
- Filter image-only lines
- Collapse blank lines

***

## Test Plan

- [x] Unit tests added
- [x] Snapshot tests pass
- [ ] Manual testing

___

<!-- Do not edit below this line -->
<!-- Auto-generated footer -->"#;

        let result = filter_markdown_body(input);

        fn count_tokens(text: &str) -> usize {
            text.split_whitespace().count()
        }

        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&result);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);

        assert!(
            savings >= 30.0,
            "Expected ≥30% savings, got {savings:.1}% (input: {input_tokens} tokens, output: {output_tokens} tokens)"
        );

        // 验证有意义的内容仍被保留
        assert!(result.contains("## Summary"));
        assert!(result.contains("## Changes"));
        assert!(result.contains("## Test Plan"));
        assert!(result.contains("Filter HTML comments"));
    }
}

use crate::tracking;
use crate::utils::ok_confirmation;
use crate::utils::resolved_command;
use crate::utils::strip_ansi;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use lazy_static::lazy_static;
use regex::Regex;
use std::ffi::OsString;

lazy_static! {
    static ref EMAIL_RE: Regex =
        crate::utils::compile_regex(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b");
    static ref BRANCH_NAME_RE: Regex = crate::utils::compile_regex(
        r#"(?:Created|Pushed|pushed|Deleted|deleted)\s+branch\s+[`"']?([a-zA-Z0-9/_.\-+@]+)"#
    );
    static ref PR_LINE_RE: Regex = crate::utils::compile_regex(
        r"(Created|Updated)\s+pull\s+request\s+#(\d+)\s+for\s+([^\s:]+)(?::\s*(\S+))?"
    );
}

fn run_gt_filtered(
    subcmd: &[&str],
    args: &[String],
    verbose: u8,
    tee_label: &str,
    filter_fn: fn(&str) -> String,
) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gt");
    for part in subcmd {
        cmd.arg(part);
    }
    for arg in args {
        cmd.arg(arg);
    }

    let subcmd_str = subcmd.join(" ");
    if verbose > 0 {
        eprintln!("运行：gt {} {}", subcmd_str, args.join(" "));
    }

    let cmd_output = cmd
        .output()
        .with_context(|| format!("运行 gt {subcmd_str} 失败。是否已安装 gt（Graphite）？"))?;

    let stdout = crate::utils::decode_output(&cmd_output.stdout);
    let stderr = crate::utils::decode_output(&cmd_output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let exit_code = cmd_output.status.code().unwrap_or(1);

    let clean = strip_ansi(stdout.trim());
    let output = if verbose > 0 {
        clean
    } else {
        filter_fn(&clean)
    };

    if let Some(hint) = crate::tee::tee_and_hint(&raw, tee_label, exit_code) {
        println!("{output}\n{hint}");
    } else {
        println!("{output}");
    }

    if !stderr.trim().is_empty() {
        eprintln!("{}", stderr.trim());
    }

    let label = if args.is_empty() {
        format!("gt {subcmd_str}")
    } else {
        format!("gt {} {}", subcmd_str, args.join(" "))
    };
    let rtk_label = format!("rtk {label}");
    timer.track(&label, &rtk_label, &raw, &output);

    if !cmd_output.status.success() {
        std::process::exit(exit_code);
    }

    Ok(())
}

fn filter_identity(input: &str) -> String {
    input.to_string()
}

pub fn run_log(args: &[String], verbose: u8) -> Result<()> {
    match args.first().map(std::string::String::as_str) {
        Some("short") => run_gt_filtered(
            &["log", "short"],
            &args[1..],
            verbose,
            "gt_log_short",
            filter_identity,
        ),
        Some("long") => run_gt_filtered(
            &["log", "long"],
            &args[1..],
            verbose,
            "gt_log_long",
            filter_gt_log_entries,
        ),
        _ => run_gt_filtered(&["log"], args, verbose, "gt_log", filter_gt_log_entries),
    }
}

pub fn run_submit(args: &[String], verbose: u8) -> Result<()> {
    run_gt_filtered(&["submit"], args, verbose, "gt_submit", filter_gt_submit)
}

pub fn run_sync(args: &[String], verbose: u8) -> Result<()> {
    run_gt_filtered(&["sync"], args, verbose, "gt_sync", filter_gt_sync)
}

pub fn run_restack(args: &[String], verbose: u8) -> Result<()> {
    run_gt_filtered(&["restack"], args, verbose, "gt_restack", filter_gt_restack)
}

pub fn run_create(args: &[String], verbose: u8) -> Result<()> {
    run_gt_filtered(&["create"], args, verbose, "gt_create", filter_gt_create)
}

pub fn run_branch(args: &[String], verbose: u8) -> Result<()> {
    run_gt_filtered(&["branch"], args, verbose, "gt_branch", filter_identity)
}

pub fn run_other(args: &[OsString], verbose: u8) -> Result<()> {
    if args.is_empty() {
        anyhow::bail!("gt：未指定子命令");
    }

    let subcommand = args[0].to_string_lossy();
    let rest: Vec<String> = args[1..]
        .iter()
        .map(|a| a.to_string_lossy().into())
        .collect();

    // gt 会把未知子命令转交给 git，因此 "gt status" 等同于 "git status"。
    // 已知 git 命令则路由到 RTK 的 git 过滤器，以节省 token。
    match subcommand.as_ref() {
        "status" => crate::git::run(
            crate::git::GitCommand::Status,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        "diff" => crate::git::run(
            crate::git::GitCommand::Diff,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        "show" => crate::git::run(
            crate::git::GitCommand::Show,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        "add" => crate::git::run(
            crate::git::GitCommand::Add,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        "push" => crate::git::run(
            crate::git::GitCommand::Push,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        "pull" => crate::git::run(
            crate::git::GitCommand::Pull,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        "fetch" => crate::git::run(
            crate::git::GitCommand::Fetch,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        "stash" => {
            let stash_sub = rest.first().cloned();
            let stash_args = rest.get(1..).unwrap_or(&[]);
            crate::git::run(
                crate::git::GitCommand::Stash {
                    subcommand: stash_sub,
                },
                stash_args,
                /*max_lines*/ None,
                verbose,
                &[],
            )
        }
        "worktree" => crate::git::run(
            crate::git::GitCommand::Worktree,
            &rest,
            /*max_lines*/ None,
            verbose,
            &[],
        ),
        _ => passthrough_gt(&subcommand, &rest, verbose),
    }
}

fn passthrough_gt(subcommand: &str, args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    let mut cmd = resolved_command("gt");
    cmd.arg(subcommand);
    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        eprintln!("运行：gt {} {}", subcommand, args.join(" "));
    }

    let status = cmd
        .status()
        .with_context(|| format!("运行 gt {subcommand} 失败"))?;

    let args_str = if args.is_empty() {
        subcommand.to_string()
    } else {
        format!("{} {}", subcommand, args.join(" "))
    };
    timer.track_passthrough(
        &format!("gt {args_str}"),
        &format!("rtk gt {args_str} (passthrough)"),
    );

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

const MAX_LOG_ENTRIES: usize = 15;

fn filter_gt_log_entries(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let lines: Vec<&str> = trimmed.lines().collect();
    let mut result = Vec::new();
    let mut entry_count = 0;

    for (i, line) in lines.iter().enumerate() {
        if is_graph_node(line) {
            entry_count += 1;
        }

        let replaced = EMAIL_RE.replace_all(line, "");
        let processed = truncate(replaced.trim_end(), /*max_len*/ 120);
        result.push(processed);

        if entry_count >= MAX_LOG_ENTRIES {
            let remaining = lines[i + 1..].iter().filter(|l| is_graph_node(l)).count();
            if remaining > 0 {
                result.push(format!("... +{remaining} 条"));
            }
            break;
        }
    }

    result.join("\n")
}

fn filter_gt_submit(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut pushed = Vec::new();
    let mut prs = Vec::new();

    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if line.contains("pushed") || line.contains("Pushed") {
            pushed.push(extract_branch_name(line));
        } else if let Some(caps) = PR_LINE_RE.captures(line) {
            let action = match caps[1].to_lowercase().as_str() {
                "created" => "创建",
                "updated" => "更新",
                _ => "更新",
            };
            let num = &caps[2];
            let branch = &caps[3];
            if let Some(url) = caps.get(4) {
                prs.push(format!(
                    "已{action} PR #{} {} {}",
                    num,
                    branch,
                    url.as_str()
                ));
            } else {
                prs.push(format!("已{action} PR #{num} {branch}"));
            }
        }
    }

    let mut summary = Vec::new();

    if !pushed.is_empty() {
        let branch_names: Vec<&str> = pushed
            .iter()
            .map(std::string::String::as_str)
            .filter(|s| !s.is_empty())
            .collect();
        if !branch_names.is_empty() {
            summary.push(format!("已推送 {}", branch_names.join(", ")));
        } else {
            summary.push(format!("已推送 {} 个分支", pushed.len()));
        }
    }

    summary.extend(prs);

    if summary.is_empty() {
        return truncate(trimmed, /*max_len*/ 200);
    }

    summary.join("\n")
}

fn filter_gt_sync(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut synced = 0;
    let mut deleted = 0;
    let mut deleted_names = Vec::new();

    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if (line.contains("Synced") && line.contains("branch"))
            || line.starts_with("Synced with remote")
        {
            synced += 1;
        }
        if line.contains("deleted") || line.contains("Deleted") {
            deleted += 1;
            let name = extract_branch_name(line);
            if !name.is_empty() {
                deleted_names.push(name);
            }
        }
    }

    let mut parts = Vec::new();

    if synced > 0 {
        parts.push(format!("已同步 {synced} 个"));
    }

    if deleted > 0 {
        if deleted_names.is_empty() {
            parts.push(format!("已删除 {deleted} 个"));
        } else {
            parts.push(format!(
                "已删除 {deleted} 个（{}）",
                deleted_names.join(", ")
            ));
        }
    }

    if parts.is_empty() {
        return ok_confirmation("同步", "");
    }

    parts.join("，")
}

fn filter_gt_restack(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut restacked = 0;
    for line in trimmed.lines() {
        let line = line.trim();
        if (line.contains("Restacked") || line.contains("Rebased")) && line.contains("branch") {
            restacked += 1;
        }
    }

    if restacked > 0 {
        ok_confirmation("重排", &format!("{restacked} 个分支"))
    } else {
        ok_confirmation("重排", "")
    }
}

fn filter_gt_create(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let branch_name = trimmed
        .lines()
        .find_map(|line| {
            let line = line.trim();
            if line.contains("Created") || line.contains("created") {
                Some(extract_branch_name(line))
            } else {
                None
            }
        })
        .unwrap_or_default();

    if branch_name.is_empty() {
        let first_line = trimmed.lines().next().unwrap_or("");
        ok_confirmation("创建", first_line.trim())
    } else {
        ok_confirmation("创建", &branch_name)
    }
}

fn is_graph_node(line: &str) -> bool {
    let stripped = line
        .trim_start_matches('│')
        .trim_start_matches('|')
        .trim_start();
    stripped.starts_with('◉')
        || stripped.starts_with('○')
        || stripped.starts_with('◯')
        || stripped.starts_with('◆')
        || stripped.starts_with('●')
        || stripped.starts_with('@')
        || stripped.starts_with('*')
}

fn extract_branch_name(line: &str) -> String {
    BRANCH_NAME_RE
        .captures(line)
        .and_then(|cap| cap.get(1))
        .map(|m| m.as_str().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn count_tokens(text: &str) -> usize {
        text.split_whitespace().count()
    }

    #[test]
    fn test_filter_gt_log_exact_format() {
        let input = r#"◉  abc1234 feat/add-auth 2d ago
│  feat(auth): add login endpoint
│
◉  def5678 feat/add-db 3d ago user@example.com
│  feat(db): add migration system
│
◉  ghi9012 main 5d ago admin@corp.io
│  chore: update dependencies
~
"#;
        let output = filter_gt_log_entries(input);
        let expected = "\
◉  abc1234 feat/add-auth 2d ago
│  feat(auth): add login endpoint
│
◉  def5678 feat/add-db 3d ago
│  feat(db): add migration system
│
◉  ghi9012 main 5d ago
│  chore: update dependencies
~";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_gt_submit_exact_format() {
        let input = r#"Pushed branch feat/add-auth
Created pull request #42 for feat/add-auth
Pushed branch feat/add-db
Updated pull request #40 for feat/add-db
"#;
        let output = filter_gt_submit(input);
        let expected = "\
已推送 feat/add-auth, feat/add-db
已创建 PR #42 feat/add-auth
已更新 PR #40 feat/add-db";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_filter_gt_sync_exact_format() {
        let input = r#"Synced with remote
Deleted branch feat/merged-feature
Deleted branch fix/old-hotfix
"#;
        let output = filter_gt_sync(input);
        assert_eq!(
            output,
            "已同步 1 个，已删除 2 个（feat/merged-feature, fix/old-hotfix）"
        );
    }

    #[test]
    fn test_filter_gt_restack_exact_format() {
        let input = r#"Restacked branch feat/add-auth on main
Restacked branch feat/add-db on feat/add-auth
Restacked branch fix/parsing on feat/add-db
"#;
        let output = filter_gt_restack(input);
        assert_eq!(output, "已重排 3 个分支");
    }

    #[test]
    fn test_filter_gt_create_exact_format() {
        let input = "Created branch feat/new-feature\n";
        let output = filter_gt_create(input);
        assert_eq!(output, "已创建 feat/new-feature");
    }

    #[test]
    fn test_filter_gt_log_truncation() {
        let mut input = String::new();
        for i in 0..20 {
            input.push_str(&format!(
                "◉  hash{i:02} branch-{i} 1d ago dev@example.com\n│  commit message {i}\n│\n"
            ));
        }
        input.push_str("~\n");

        let output = filter_gt_log_entries(&input);
        assert!(output.contains("... +"));
    }

    #[test]
    fn test_filter_gt_log_empty() {
        assert_eq!(filter_gt_log_entries(""), String::new());
        assert_eq!(filter_gt_log_entries("  "), String::new());
    }

    #[test]
    fn test_filter_gt_log_token_savings() {
        let mut input = String::new();
        for i in 0..40 {
            input.push_str(&format!(
                "◉  hash{:02}abc feat/feature-{} {}d ago developer{}@longcompany.example.com\n\
                 │  feat(module-{}): implement feature {} with detailed description of changes\n│\n",
                i,
                i,
                i + 1,
                i,
                i,
                i
            ));
        }
        input.push_str("~\n");

        let output = filter_gt_log_entries(&input);
        let input_tokens = count_tokens(&input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "gt log 过滤器预期至少节省 60% token，实际为 {savings:.1}%（{input_tokens} -> {output_tokens}）"
        );
    }

    #[test]
    fn test_filter_gt_log_long() {
        let input = r#"◉  abc1234 feat/add-auth
│  Author: Dev User <dev@example.com>
│  Date: 2026-02-25 10:30:00 -0800
│
│  feat(auth): add login endpoint with OAuth2 support
│  and session management for web clients
│
◉  def5678 feat/add-db
│  Author: Other Dev <other@example.com>
│  Date: 2026-02-24 14:00:00 -0800
│
│  feat(db): add migration system
~
"#;

        let output = filter_gt_log_entries(input);
        assert!(output.contains("abc1234"));
        assert!(!output.contains("dev@example.com"));
        assert!(!output.contains("other@example.com"));
    }

    #[test]
    fn test_filter_gt_submit_empty() {
        assert_eq!(filter_gt_submit(""), String::new());
    }

    #[test]
    fn test_filter_gt_submit_with_urls() {
        let input =
            "Created pull request #42 for feat/add-auth: https://github.com/org/repo/pull/42\n";
        let output = filter_gt_submit(input);
        assert!(output.contains("PR #42"));
        assert!(output.contains("feat/add-auth"));
        assert!(output.contains("https://github.com/org/repo/pull/42"));
    }

    #[test]
    fn test_filter_gt_submit_token_savings() {
        let input = r#"
  ✅  Pushing to remote...
  Enumerating objects: 15, done.
  Counting objects: 100% (15/15), done.
  Delta compression using up to 10 threads
  Compressing objects: 100% (8/8), done.
  Writing objects: 100% (10/10), 2.50 KiB | 2.50 MiB/s, done.
  Total 10 (delta 5), reused 0 (delta 0), pack-reused 0
  Pushed branch feat/add-auth to origin
  Creating pull request for feat/add-auth...
  Created pull request #42 for feat/add-auth: https://github.com/org/repo/pull/42
  ✅  Pushing to remote...
  Enumerating objects: 8, done.
  Counting objects: 100% (8/8), done.
  Delta compression using up to 10 threads
  Compressing objects: 100% (4/4), done.
  Writing objects: 100% (5/5), 1.20 KiB | 1.20 MiB/s, done.
  Total 5 (delta 3), reused 0 (delta 0), pack-reused 0
  Pushed branch feat/add-db to origin
  Updating pull request for feat/add-db...
  Updated pull request #40 for feat/add-db: https://github.com/org/repo/pull/40
  ✅  Pushing to remote...
  Enumerating objects: 5, done.
  Counting objects: 100% (5/5), done.
  Delta compression using up to 10 threads
  Compressing objects: 100% (3/3), done.
  Writing objects: 100% (3/3), 890 bytes | 890.00 KiB/s, done.
  Total 3 (delta 2), reused 0 (delta 0), pack-reused 0
  Pushed branch fix/parsing to origin
  All branches submitted successfully!
"#;

        let output = filter_gt_submit(input);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "gt submit 过滤器预期至少节省 60% token，实际为 {savings:.1}%（{input_tokens} -> {output_tokens}）"
        );
    }

    #[test]
    fn test_filter_gt_sync() {
        let input = r#"Synced with remote
Deleted branch feat/merged-feature
Deleted branch fix/old-hotfix
"#;

        let output = filter_gt_sync(input);
        assert!(output.contains("已同步"));
        assert!(output.contains("已删除"));
    }

    #[test]
    fn test_filter_gt_sync_empty() {
        assert_eq!(filter_gt_sync(""), String::new());
    }

    #[test]
    fn test_filter_gt_sync_no_deletes() {
        let input = "Synced with remote\n";
        let output = filter_gt_sync(input);
        assert!(output.contains("已同步"));
        assert!(!output.contains("已删除"));
    }

    #[test]
    fn test_filter_gt_restack() {
        let input = r#"Restacked branch feat/add-auth on main
Restacked branch feat/add-db on feat/add-auth
Restacked branch fix/parsing on feat/add-db
"#;

        let output = filter_gt_restack(input);
        assert!(output.contains("已重排"));
        assert!(output.contains("3 个分支"));
    }

    #[test]
    fn test_filter_gt_restack_empty() {
        assert_eq!(filter_gt_restack(""), String::new());
    }

    #[test]
    fn test_filter_gt_create() {
        let input = "Created branch feat/new-feature\n";
        let output = filter_gt_create(input);
        assert_eq!(output, "已创建 feat/new-feature");
    }

    #[test]
    fn test_filter_gt_create_empty() {
        assert_eq!(filter_gt_create(""), String::new());
    }

    #[test]
    fn test_filter_gt_create_no_branch_name() {
        let input = "Some unexpected output\n";
        let output = filter_gt_create(input);
        assert!(output.starts_with("已创建"));
    }

    #[test]
    fn test_is_graph_node() {
        assert!(is_graph_node("◉  abc1234 main"));
        assert!(is_graph_node("○  def5678 feat/x"));
        assert!(is_graph_node("@  ghi9012 (current)"));
        assert!(is_graph_node("*  jkl3456 branch"));
        assert!(is_graph_node("│ ◉  nested node"));
        assert!(!is_graph_node("│  just a message line"));
        assert!(!is_graph_node("~"));
    }

    #[test]
    fn test_extract_branch_name() {
        assert_eq!(
            extract_branch_name("Created branch feat/new-feature"),
            "feat/new-feature"
        );
        assert_eq!(
            extract_branch_name("Pushed branch fix/bug-123"),
            "fix/bug-123"
        );
        assert_eq!(
            extract_branch_name("Pushed branch feat/auth+session"),
            "feat/auth+session"
        );
        assert_eq!(extract_branch_name("Created branch user@fix"), "user@fix");
        assert_eq!(extract_branch_name("no branch here"), "");
    }

    #[test]
    fn test_filter_gt_log_pre_stripped_input() {
        let input = "◉  abc1234 feat/x 1d ago user@test.com\n│  message\n~\n";
        let output = filter_gt_log_entries(input);
        assert!(output.contains("abc1234"));
        assert!(!output.contains("user@test.com"));
    }

    #[test]
    fn test_filter_gt_sync_token_savings() {
        let input = r#"
  ✅ Syncing with remote...
  Pulling latest changes from main...
  Successfully pulled 5 new commits
  Synced branch feat/add-auth with remote
  Synced branch feat/add-db with remote
  Branch feat/merged-feature has been merged
  Deleted branch feat/merged-feature
  Branch fix/old-hotfix has been merged
  Deleted branch fix/old-hotfix
  All branches synced!
"#;

        let output = filter_gt_sync(input);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "gt sync 过滤器预期至少节省 60% token，实际为 {savings:.1}%（{input_tokens} -> {output_tokens}）"
        );
    }

    #[test]
    fn test_filter_gt_create_token_savings() {
        let input = r#"
  ✅ Creating new branch...
  Checking out from feat/add-auth...
  Created branch feat/new-feature from feat/add-auth
  Tracking branch set up to follow feat/add-auth
  Branch feat/new-feature is ready for development
"#;

        let output = filter_gt_create(input);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "gt create 过滤器预期至少节省 60% token，实际为 {savings:.1}%（{input_tokens} -> {output_tokens}）"
        );
    }

    #[test]
    fn test_filter_gt_restack_token_savings() {
        let input = r#"
  ✅ Restacking branches...
  Restacked branch feat/add-auth on top of main
  Successfully rebased feat/add-auth (3 commits)
  Restacked branch feat/add-db on top of feat/add-auth
  Successfully rebased feat/add-db (2 commits)
  Restacked branch fix/parsing on top of feat/add-db
  Successfully rebased fix/parsing (1 commit)
  All branches restacked!
"#;

        let output = filter_gt_restack(input);
        let input_tokens = count_tokens(input);
        let output_tokens = count_tokens(&output);
        let savings = 100.0 - (output_tokens as f64 / input_tokens as f64 * 100.0);
        assert!(
            savings >= 60.0,
            "gt restack 过滤器预期至少节省 60% token，实际为 {savings:.1}%"
        );
    }
}

use crate::tracking;
use crate::utils::resolved_command;
use crate::utils::tool_exists;
use crate::utils::truncate;
use anyhow::Context;
use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;

pub fn run(args: &[String], verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    // Try tsc directly first, fallback to npx if not found
    let tsc_exists = tool_exists("tsc");

    let mut cmd = if tsc_exists {
        resolved_command("tsc")
    } else {
        let mut c = resolved_command("npx");
        c.arg("tsc");
        c
    };

    for arg in args {
        cmd.arg(arg);
    }

    if verbose > 0 {
        let tool = if tsc_exists { "tsc" } else { "npx tsc" };
        eprintln!("运行：{tool} {}", args.join(" "));
    }

    let output = cmd
        .output()
        .context("运行 tsc 失败（可尝试：npm install -g typescript）")?;
    let stdout = crate::utils::decode_output(&output.stdout);
    let stderr = crate::utils::decode_output(&output.stderr);
    let raw = format!("{stdout}\n{stderr}");

    let filtered = filter_tsc_output(&raw);

    let exit_code = output.status.code().unwrap_or(1);
    if let Some(hint) = crate::tee::tee_and_hint(&raw, "tsc", exit_code) {
        println!("{filtered}\n{hint}");
    } else {
        println!("{filtered}");
    }

    timer.track(
        &format!("tsc {}", args.join(" ")),
        &format!("rtk tsc {}", args.join(" ")),
        &raw,
        &filtered,
    );

    // Preserve tsc exit code for CI/CD compatibility
    std::process::exit(exit_code);
}

/// Filter TypeScript compiler output - group errors by file, show every error
fn filter_tsc_output(output: &str) -> String {
    lazy_static::lazy_static! {
        // Pattern: src/file.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.
        static ref TSC_ERROR: Regex = crate::utils::compile_regex(
            r"^(.+?)\((\d+),(\d+)\):\s+(error|warning)\s+(TS\d+):\s+(.+)$"
        );
    }

    struct TsError {
        file: String,
        line: usize,
        code: String,
        message: String,
        context_lines: Vec<String>,
    }

    let mut errors: Vec<TsError> = Vec::new();
    let lines: Vec<&str> = output.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        if let Some(caps) = TSC_ERROR.captures(line) {
            let mut err = TsError {
                file: caps[1].to_string(),
                line: caps[2].parse().unwrap_or(0),
                code: caps[5].to_string(),
                message: caps[6].to_string(),
                context_lines: Vec::new(),
            };

            // Capture continuation lines (indented context from tsc)
            i += 1;
            while i < lines.len() {
                let next = lines[i];
                if !next.is_empty()
                    && (next.starts_with("  ") || next.starts_with('\t'))
                    && !TSC_ERROR.is_match(next)
                {
                    err.context_lines.push(next.trim().to_string());
                    i += 1;
                } else {
                    break;
                }
            }

            errors.push(err);
        } else {
            i += 1;
        }
    }

    if errors.is_empty() {
        if output.contains("Found 0 errors") {
            return "✓ TypeScript：未发现错误".to_string();
        }
        return "TypeScript 编译完成".to_string();
    }

    // Group by file
    let mut by_file: HashMap<String, Vec<&TsError>> = HashMap::new();
    for err in &errors {
        by_file.entry(err.file.clone()).or_default().push(err);
    }

    // Count by error code for summary
    let mut by_code: HashMap<String, usize> = HashMap::new();
    for err in &errors {
        *by_code.entry(err.code.clone()).or_insert(0) += 1;
    }

    let mut result = String::new();
    result.push_str(&format!(
        "TypeScript：{} 个错误，{} 个文件\n",
        errors.len(),
        by_file.len()
    ));
    result.push_str("═══════════════════════════════════════\n");

    // Top error codes summary (compact, one line)
    let mut code_counts: Vec<_> = by_code.iter().collect();
    code_counts.sort_by(|a, b| b.1.cmp(a.1));

    if code_counts.len() > 1 {
        let codes_str: Vec<String> = code_counts
            .iter()
            .take(5)
            .map(|(code, count)| format!("{code} ({count}x)"))
            .collect();
        result.push_str(&format!("错误码：{}\n\n", codes_str.join(", ")));
    }

    // Files sorted by error count (most errors first)
    let mut files_sorted: Vec<_> = by_file.iter().collect();
    files_sorted.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    // Show every error per file — no limits
    for (file, file_errors) in &files_sorted {
        result.push_str(&format!("{}（{} 个错误）\n", file, file_errors.len()));

        for err in *file_errors {
            result.push_str(&format!(
                "  行{}：{} {}\n",
                err.line,
                err.code,
                truncate(&err.message, /*max_len*/ 120)
            ));
            for ctx in &err.context_lines {
                result.push_str(&format!("    {}\n", truncate(ctx, /*max_len*/ 120)));
            }
        }
        result.push('\n');
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_tsc_output() {
        let output = r#"
src/server/api/auth.ts(12,5): error TS2322: Type 'string' is not assignable to type 'number'.
src/server/api/auth.ts(15,10): error TS2345: Argument of type 'number' is not assignable to parameter of type 'string'.
src/components/Button.tsx(8,3): error TS2339: Property 'onClick' does not exist on type 'ButtonProps'.
src/components/Button.tsx(10,5): error TS2322: Type 'string' is not assignable to type 'number'.

Found 4 errors in 2 files.
"#;
        let result = filter_tsc_output(output);
        assert!(result.contains("TypeScript：4 个错误，2 个文件"));
        assert!(result.contains("auth.ts（2 个错误）"));
        assert!(result.contains("Button.tsx（2 个错误）"));
        assert!(result.contains("TS2322"));
        assert!(!result.contains("Found 4 errors")); // Summary line should be replaced
    }

    #[test]
    fn test_every_error_message_shown() {
        let output = "\
src/api.ts(10,5): error TS2322: Type 'string' is not assignable to type 'number'.
src/api.ts(20,5): error TS2322: Type 'boolean' is not assignable to type 'string'.
src/api.ts(30,5): error TS2322: Type 'null' is not assignable to type 'object'.
";
        let result = filter_tsc_output(output);
        // Each error message must be individually visible, not collapsed
        assert!(result.contains("Type 'string' is not assignable to type 'number'"));
        assert!(result.contains("Type 'boolean' is not assignable to type 'string'"));
        assert!(result.contains("Type 'null' is not assignable to type 'object'"));
        assert!(result.contains("行10："));
        assert!(result.contains("行20："));
        assert!(result.contains("行30："));
    }

    #[test]
    fn test_continuation_lines_preserved() {
        let output = "\
src/app.tsx(10,3): error TS2322: Type '{ children: Element; }' is not assignable to type 'Props'.
  Property 'children' does not exist on type 'Props'.
src/app.tsx(20,5): error TS2345: Argument of type 'number' is not assignable to parameter of type 'string'.
";
        let result = filter_tsc_output(output);
        assert!(result.contains("Property 'children' does not exist on type 'Props'"));
        assert!(result.contains("行10："));
        assert!(result.contains("行20："));
    }

    #[test]
    fn test_no_file_limit() {
        // 15 files with errors — all must appear
        let mut output = String::new();
        for i in 1..=15 {
            output.push_str(&format!(
                "src/file{i}.ts({i},1): error TS2322: Error in file {i}.\n"
            ));
        }
        let result = filter_tsc_output(&output);
        assert!(result.contains("15 个错误，15 个文件"));
        for i in 1..=15 {
            assert!(
                result.contains(&format!("file{i}.ts")),
                "file{i}.ts missing from output"
            );
        }
    }

    #[test]
    fn test_filter_no_errors() {
        let output = "Found 0 errors. Watching for file changes.";
        let result = filter_tsc_output(output);
        assert!(result.contains("未发现错误"));
    }
}

use crate::tracking;
use crate::utils::truncate;
use anyhow::Result;
use std::fs;
use std::path::Path;

/// 超浓缩 diff：只保留变更行，不带上下文
pub fn run(file1: &Path, file2: &Path, verbose: u8) -> Result<()> {
    let timer = tracking::TimedExecution::start();

    if verbose > 0 {
        eprintln!("比较：{} vs {}", file1.display(), file2.display());
    }

    let content1 = fs::read_to_string(file1)?;
    let content2 = fs::read_to_string(file2)?;
    let raw = format!("{content1}\n---\n{content2}");

    let lines1: Vec<&str> = content1.lines().collect();
    let lines2: Vec<&str> = content2.lines().collect();
    let diff = compute_diff(&lines1, &lines2);
    let mut rtk = String::new();

    if diff.added == 0 && diff.removed == 0 {
        rtk.push_str("✅ 文件完全一致");
        println!("{rtk}");
        timer.track(
            &format!("diff {} {}", file1.display(), file2.display()),
            "rtk diff",
            &raw,
            &rtk,
        );
        return Ok(());
    }

    rtk.push_str(&format!("📊 {} → {}\n", file1.display(), file2.display()));
    rtk.push_str(&format!(
        "   +{} 新增，-{} 删除，~{} 修改\n\n",
        diff.added, diff.removed, diff.modified
    ));

    for change in diff.changes.iter().take(50) {
        match change {
            DiffChange::Added(ln, c) => {
                rtk.push_str(&format!("+{:4} {}\n", ln, truncate(c, /*max_len*/ 80)))
            }
            DiffChange::Removed(ln, c) => {
                rtk.push_str(&format!("-{:4} {}\n", ln, truncate(c, /*max_len*/ 80)))
            }
            DiffChange::Modified(ln, old, new) => rtk.push_str(&format!(
                "~{:4} {} → {}\n",
                ln,
                truncate(old, /*max_len*/ 70),
                truncate(new, /*max_len*/ 70)
            )),
        }
    }
    if diff.changes.len() > 50 {
        rtk.push_str(&format!("... +{} 处变更", diff.changes.len() - 50));
    }

    print!("{rtk}");
    timer.track(
        &format!("diff {} {}", file1.display(), file2.display()),
        "rtk diff",
        &raw,
        &rtk,
    );
    Ok(())
}

/// 从 stdin 运行 diff（处理管道输入）
pub fn run_stdin(_verbose: u8) -> Result<()> {
    use std::io::Read;
    use std::io::{self};
    let timer = tracking::TimedExecution::start();

    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;

    // 解析 unified diff 格式
    let condensed = condense_unified_diff(&input);
    println!("{condensed}");

    timer.track("diff (stdin)", "rtk diff (stdin)", &input, &condensed);

    Ok(())
}

#[derive(Debug)]
enum DiffChange {
    Added(usize, String),
    Removed(usize, String),
    Modified(usize, String, String),
}

struct DiffResult {
    added: usize,
    removed: usize,
    modified: usize,
    changes: Vec<DiffChange>,
}

fn compute_diff(lines1: &[&str], lines2: &[&str]) -> DiffResult {
    let mut changes = Vec::new();
    let mut added = 0;
    let mut removed = 0;
    let mut modified = 0;

    // 简单逐行比较（不一定最优，但足够快）
    let max_len = lines1.len().max(lines2.len());

    for i in 0..max_len {
        let l1 = lines1.get(i).copied();
        let l2 = lines2.get(i).copied();

        match (l1, l2) {
            (Some(a), Some(b)) if a != b => {
                // 判断是相似修改，还是完全不同
                if similarity(a, b) > 0.5 {
                    changes.push(DiffChange::Modified(i + 1, a.to_string(), b.to_string()));
                    modified += 1;
                } else {
                    changes.push(DiffChange::Removed(i + 1, a.to_string()));
                    changes.push(DiffChange::Added(i + 1, b.to_string()));
                    removed += 1;
                    added += 1;
                }
            }
            (Some(a), None) => {
                changes.push(DiffChange::Removed(i + 1, a.to_string()));
                removed += 1;
            }
            (None, Some(b)) => {
                changes.push(DiffChange::Added(i + 1, b.to_string()));
                added += 1;
            }
            _ => {}
        }
    }

    DiffResult {
        added,
        removed,
        modified,
        changes,
    }
}

fn similarity(a: &str, b: &str) -> f64 {
    let a_chars: std::collections::HashSet<char> = a.chars().collect();
    let b_chars: std::collections::HashSet<char> = b.chars().collect();

    let intersection = a_chars.intersection(&b_chars).count();
    let union = a_chars.union(&b_chars).count();

    if union == 0 {
        1.0
    } else {
        intersection as f64 / union as f64
    }
}

fn condense_unified_diff(diff: &str) -> String {
    let mut result = Vec::new();
    let mut current_file = String::new();
    let mut added = 0;
    let mut removed = 0;
    let mut changes = Vec::new();

    for line in diff.lines() {
        if line.starts_with("diff --git") || line.starts_with("--- ") || line.starts_with("+++ ") {
            // 文件头
            if line.starts_with("+++ ") {
                if !current_file.is_empty() && (added > 0 || removed > 0) {
                    result.push(format!("📄 {current_file} (+{added} -{removed})"));
                    for c in changes.iter().take(10) {
                        result.push(format!("  {c}"));
                    }
                    if changes.len() > 10 {
                        result.push(format!("  ... +{} 处更多变更", changes.len() - 10));
                    }
                }
                current_file = line
                    .trim_start_matches("+++ ")
                    .trim_start_matches("b/")
                    .to_string();
                added = 0;
                removed = 0;
                changes.clear();
            }
        } else if line.starts_with('+') && !line.starts_with("+++") {
            added += 1;
            if changes.len() < 15 {
                changes.push(truncate(line, /*max_len*/ 70));
            }
        } else if line.starts_with('-') && !line.starts_with("---") {
            removed += 1;
            if changes.len() < 15 {
                changes.push(truncate(line, /*max_len*/ 70));
            }
        }
    }

    // 最后一个文件
    if !current_file.is_empty() && (added > 0 || removed > 0) {
        result.push(format!("📄 {current_file} (+{added} -{removed})"));
        for c in changes.iter().take(10) {
            result.push(format!("  {c}"));
        }
        if changes.len() > 10 {
            result.push(format!("  ... +{} 处更多变更", changes.len() - 10));
        }
    }

    result.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- similarity ---

    #[test]
    fn test_similarity_identical() {
        assert_eq!(similarity("hello", "hello"), 1.0);
    }

    #[test]
    fn test_similarity_completely_different() {
        assert_eq!(similarity("abc", "xyz"), 0.0);
    }

    #[test]
    fn test_similarity_empty_strings() {
        // 两边都为空：并集为 0，按约定返回 1.0
        assert_eq!(similarity("", ""), 1.0);
    }

    #[test]
    fn test_similarity_partial_overlap() {
        let s = similarity("abcd", "abef");
        // 交集为 a、b；并集为 a、b、c、d、e、f，共 6 个，因此 Jaccard = 2/6
        assert!((s - 2.0 / 6.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_similarity_threshold_for_modified() {
        // "let x = 1;" 与 "let x = 2;" 的相似度应 > 0.5（视为修改）
        assert!(similarity("let x = 1;", "let x = 2;") > 0.5);
    }

    // --- truncate ---

    #[test]
    fn test_truncate_short_string() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string() {
        assert_eq!(truncate("hello world!", 8), "hello...");
    }

    // --- compute_diff ---

    #[test]
    fn test_compute_diff_identical() {
        let a = vec!["line1", "line2", "line3"];
        let b = vec!["line1", "line2", "line3"];
        let result = compute_diff(&a, &b);
        assert_eq!(result.added, 0);
        assert_eq!(result.removed, 0);
        assert_eq!(result.modified, 0);
        assert!(result.changes.is_empty());
    }

    #[test]
    fn test_compute_diff_added_lines() {
        let a = vec!["line1"];
        let b = vec!["line1", "line2", "line3"];
        let result = compute_diff(&a, &b);
        assert_eq!(result.added, 2);
        assert_eq!(result.removed, 0);
    }

    #[test]
    fn test_compute_diff_removed_lines() {
        let a = vec!["line1", "line2", "line3"];
        let b = vec!["line1"];
        let result = compute_diff(&a, &b);
        assert_eq!(result.removed, 2);
        assert_eq!(result.added, 0);
    }

    #[test]
    fn test_compute_diff_modified_line() {
        // 相似行（相似度 > 0.5）应归类为修改
        let a = vec!["let x = 1;"];
        let b = vec!["let x = 2;"];
        let result = compute_diff(&a, &b);
        assert_eq!(result.modified, 1);
        assert_eq!(result.added, 0);
        assert_eq!(result.removed, 0);
    }

    #[test]
    fn test_compute_diff_completely_different_line() {
        // 不相似的行（相似度 <= 0.5）应视为新增+删除，而非修改
        let a = vec!["aaaa"];
        let b = vec!["zzzz"];
        let result = compute_diff(&a, &b);
        assert_eq!(result.modified, 0);
        assert_eq!(result.added, 1);
        assert_eq!(result.removed, 1);
    }

    #[test]
    fn test_compute_diff_empty_inputs() {
        let result = compute_diff(&[], &[]);
        assert_eq!(result.added, 0);
        assert_eq!(result.removed, 0);
        assert!(result.changes.is_empty());
    }

    // --- condense_unified_diff ---

    #[test]
    fn test_condense_unified_diff_single_file() {
        let diff = r#"diff --git a/src/main.rs b/src/main.rs
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,3 +1,4 @@
 fn main() {
+    println!("hello");
     println!("world");
 }
"#;
        let result = condense_unified_diff(diff);
        assert!(result.contains("src/main.rs"));
        assert!(result.contains("+1"));
        assert!(result.contains("println"));
    }

    #[test]
    fn test_condense_unified_diff_multiple_files() {
        let diff = r#"diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
+added line
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
-removed line
"#;
        let result = condense_unified_diff(diff);
        assert!(result.contains("a.rs"));
        assert!(result.contains("b.rs"));
    }

    #[test]
    fn test_condense_unified_diff_empty() {
        let result = condense_unified_diff("");
        assert!(result.is_empty());
    }
}

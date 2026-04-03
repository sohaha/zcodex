/// 标准类型的 token 高效格式化 trait
use super::types::*;

/// 输出格式模式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatMode {
    /// 紧凑：仅显示摘要（默认）
    Compact,
    /// 详细：包含更多细节
    Verbose,
    /// 极简：使用符号和缩写
    Ultra,
}

impl FormatMode {
    pub fn from_verbosity(verbosity: u8) -> Self {
        match verbosity {
            0 => FormatMode::Compact,
            1 => FormatMode::Verbose,
            _ => FormatMode::Ultra,
        }
    }
}

/// 将标准类型格式化为 token 高效字符串的 trait
pub trait TokenFormatter {
    /// 格式化为紧凑摘要（默认）
    fn format_compact(&self) -> String;

    /// 格式化为详细模式
    fn format_verbose(&self) -> String;

    /// 格式化为符号压缩模式
    fn format_ultra(&self) -> String;

    /// 按指定模式格式化
    fn format(&self, mode: FormatMode) -> String {
        match mode {
            FormatMode::Compact => self.format_compact(),
            FormatMode::Verbose => self.format_verbose(),
            FormatMode::Ultra => self.format_ultra(),
        }
    }
}

impl TokenFormatter for TestResult {
    fn format_compact(&self) -> String {
        let mut lines = vec![format!("通过 ({}) 失败 ({})", self.passed, self.failed)];

        if !self.failures.is_empty() {
            lines.push(String::new());
            for (idx, failure) in self.failures.iter().enumerate().take(5) {
                lines.push(format!("{}. {}", idx + 1, failure.test_name));
                let error_preview: String = failure
                    .error_message
                    .lines()
                    .take(2)
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(format!("   {error_preview}"));
            }

            if self.failures.len() > 5 {
                lines.push(format!("\n... +{} 项失败", self.failures.len() - 5));
            }
        }

        if let Some(duration) = self.duration_ms {
            lines.push(format!("\n耗时：{duration}ms"));
        }

        lines.join("\n")
    }

    fn format_verbose(&self) -> String {
        let mut lines = vec![format!(
            "测试：{} 通过，{} 失败，{} 跳过（共 {}）",
            self.passed, self.failed, self.skipped, self.total
        )];

        if !self.failures.is_empty() {
            lines.push("\n失败：".to_string());
            for (idx, failure) in self.failures.iter().enumerate() {
                lines.push(format!(
                    "\n{}. {} ({})",
                    idx + 1,
                    failure.test_name,
                    failure.file_path
                ));
                lines.push(format!("   {}", failure.error_message));
                if let Some(stack) = &failure.stack_trace {
                    let stack_preview: String =
                        stack.lines().take(3).collect::<Vec<_>>().join("\n   ");
                    lines.push(format!("   {stack_preview}"));
                }
            }
        }

        if let Some(duration) = self.duration_ms {
            lines.push(format!("\n耗时：{duration}ms"));
        }

        lines.join("\n")
    }

    fn format_ultra(&self) -> String {
        format!(
            "✓{} ✗{} ⊘{} ({}ms)",
            self.passed,
            self.failed,
            self.skipped,
            self.duration_ms.unwrap_or(0)
        )
    }
}

impl TokenFormatter for LintResult {
    fn format_compact(&self) -> String {
        let mut lines = vec![format!(
            "错误：{} | 警告：{} | 文件：{}",
            self.errors, self.warnings, self.files_with_issues
        )];

        if !self.issues.is_empty() {
            // 按 rule_id 分组
            let mut by_rule: std::collections::HashMap<String, Vec<&LintIssue>> =
                std::collections::HashMap::new();
            for issue in &self.issues {
                by_rule
                    .entry(issue.rule_id.clone())
                    .or_default()
                    .push(issue);
            }

            let mut rules: Vec<_> = by_rule.iter().collect();
            rules.sort_by_key(|(_, issues)| std::cmp::Reverse(issues.len()));

            lines.push(String::new());
            for (rule, issues) in rules.iter().take(5) {
                lines.push(format!("{}：{} 次", rule, issues.len()));
                for issue in issues.iter().take(2) {
                    lines.push(format!("  {}:{}", issue.file_path, issue.line));
                }
            }

            if by_rule.len() > 5 {
                lines.push(format!("\n... +{} 条规则", by_rule.len() - 5));
            }
        }

        lines.join("\n")
    }

    fn format_verbose(&self) -> String {
        let mut lines = vec![format!(
            "问题总数：{}（{} 错误，{} 警告），涉及 {} 个文件",
            self.total_issues, self.errors, self.warnings, self.files_with_issues
        )];

        if !self.issues.is_empty() {
            lines.push("\n问题：".to_string());
            for issue in self.issues.iter().take(20) {
                let severity_symbol = match issue.severity {
                    LintSeverity::Error => "✗",
                    LintSeverity::Warning => "⚠",
                    LintSeverity::Info => "ℹ",
                };
                lines.push(format!(
                    "{} {}:{}:{} [{}] {}",
                    severity_symbol,
                    issue.file_path,
                    issue.line,
                    issue.column,
                    issue.rule_id,
                    issue.message
                ));
            }

            if self.issues.len() > 20 {
                lines.push(format!("\n... +{} 个问题", self.issues.len() - 20));
            }
        }

        lines.join("\n")
    }

    fn format_ultra(&self) -> String {
        format!(
            "✗{} ⚠{} 📁{}",
            self.errors, self.warnings, self.files_with_issues
        )
    }
}

impl TokenFormatter for DependencyState {
    fn format_compact(&self) -> String {
        if self.outdated_count == 0 {
            return "所有包均为最新 ✓".to_string();
        }

        let mut lines = vec![format!(
            "{} 个过期包（共 {}）",
            self.outdated_count, self.total_packages
        )];

        for dep in self.dependencies.iter().take(10) {
            if let Some(latest) = &dep.latest_version
                && &dep.current_version != latest
            {
                lines.push(format!(
                    "{}: {} → {}",
                    dep.name, dep.current_version, latest
                ));
            }
        }

        if self.outdated_count > 10 {
            lines.push(format!("\n... +{} 个", self.outdated_count - 10));
        }

        lines.join("\n")
    }

    fn format_verbose(&self) -> String {
        let mut lines = vec![format!(
            "包总数：{}（{} 个过期）",
            self.total_packages, self.outdated_count
        )];

        if self.outdated_count > 0 {
            lines.push("\n过期包：".to_string());
            for dep in &self.dependencies {
                if let Some(latest) = &dep.latest_version
                    && &dep.current_version != latest
                {
                    let dev_marker = if dep.dev_dependency { "（dev）" } else { "" };
                    lines.push(format!(
                        "  {}: {} → {}{}",
                        dep.name, dep.current_version, latest, dev_marker
                    ));
                    if let Some(wanted) = &dep.wanted_version
                        && wanted != latest
                    {
                        lines.push(format!("    （期望：{wanted}）"));
                    }
                }
            }
        }

        lines.join("\n")
    }

    fn format_ultra(&self) -> String {
        format!("📦{} ⬆️{}", self.total_packages, self.outdated_count)
    }
}

impl TokenFormatter for BuildOutput {
    fn format_compact(&self) -> String {
        let status = if self.success { "✓" } else { "✗" };
        let mut lines = vec![format!(
            "{} 构建：{} 错误，{} 警告",
            status, self.errors, self.warnings
        )];

        if !self.bundles.is_empty() {
            let total_size: u64 = self.bundles.iter().map(|b| b.size_bytes).sum();
            lines.push(format!(
                "Bundles: {}（{:.1} KB）",
                self.bundles.len(),
                total_size as f64 / 1024.0
            ));
        }

        if !self.routes.is_empty() {
            lines.push(format!("路由：{}", self.routes.len()));
        }

        if let Some(duration) = self.duration_ms {
            lines.push(format!("耗时：{duration}ms"));
        }

        lines.join("\n")
    }

    fn format_verbose(&self) -> String {
        let status = if self.success { "成功" } else { "失败" };
        let mut lines = vec![format!(
            "构建{status}：{} 错误，{} 警告",
            self.errors, self.warnings
        )];

        if !self.bundles.is_empty() {
            lines.push("\nBundles:".to_string());
            for bundle in &self.bundles {
                let gzip_info = bundle
                    .gzip_size_bytes
                    .map(|gz| format!(" (gzip: {:.1} KB)", gz as f64 / 1024.0))
                    .unwrap_or_default();
                lines.push(format!(
                    "  {}: {:.1} KB{}",
                    bundle.name,
                    bundle.size_bytes as f64 / 1024.0,
                    gzip_info
                ));
            }
        }

        if !self.routes.is_empty() {
            lines.push("\n路由：".to_string());
            for route in self.routes.iter().take(10) {
                lines.push(format!("  {}: {:.1} KB", route.path, route.size_kb));
            }
            if self.routes.len() > 10 {
                lines.push(format!("  ... +{} 条路由", self.routes.len() - 10));
            }
        }

        if let Some(duration) = self.duration_ms {
            lines.push(format!("\n耗时：{duration}ms"));
        }

        lines.join("\n")
    }

    fn format_ultra(&self) -> String {
        let status = if self.success { "✓" } else { "✗" };
        format!(
            "{} ✗{} ⚠{} ({}ms)",
            status,
            self.errors,
            self.warnings,
            self.duration_ms.unwrap_or(0)
        )
    }
}

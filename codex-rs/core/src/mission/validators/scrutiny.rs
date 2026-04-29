//! Scrutiny Validator - 代码审查验证器。
//!
//! 通过静态分析验证代码质量。

use crate::mission::handoff::CodeReviewResult;
use crate::mission::handoff::Handoff;
use crate::mission::handoff::ReviewStatus;
use crate::mission::validators::Validator;
use crate::mission::validators::ValidatorConfig;
use serde::Deserialize;
use serde::Serialize;

/// Scrutiny 验证报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrutinyReport {
    /// 总体状态。
    pub overall_status: ScrutinyStatus,
    /// 发现的问题列表。
    pub issues: Vec<Issue>,
    /// 正面发现列表。
    pub positive_findings: Vec<String>,
    /// 总体建议。
    pub recommendations: Vec<String>,
}

/// Scrutiny 总体状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScrutinyStatus {
    /// 通过。
    Passed,
    /// 失败。
    Failed,
    /// 部分通过。
    Partial,
}

impl ScrutinyStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Failed => "FAILED",
            Self::Partial => "PARTIAL",
        }
    }
}

/// 问题严重程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// 关键问题。
    Critical,
    /// 高优先级问题。
    High,
    /// 中等优先级问题。
    Medium,
    /// 低优先级问题。
    Low,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

/// 发现的问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Issue {
    /// 问题标题。
    pub title: String,
    /// 严重程度。
    pub severity: Severity,
    /// 文件路径。
    pub location: Option<String>,
    /// 问题描述。
    pub description: String,
    /// 影响说明。
    pub impact: String,
    /// 修复建议。
    pub recommendation: String,
}

/// Scrutiny 验证器。
#[derive(Debug, Clone)]
pub struct ScrutinyValidator {
    config: ValidatorConfig,
}

impl ScrutinyValidator {
    /// 创建新的 Scrutiny 验证器。
    pub fn new(config: ValidatorConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建验证器。
    pub fn with_defaults() -> Self {
        Self::new(ValidatorConfig::default())
    }

    /// 生成 Markdown 格式报告。
    pub fn report_as_markdown(&self, report: &ScrutinyReport) -> String {
        let mut output = String::new();

        output.push_str("# Scrutiny Validation Report\n\n");

        output.push_str(&format!(
            "**Overall Status:** {}\n",
            report.overall_status.label()
        ));
        output.push_str(&format!("**Issues Found:** {}\n", report.issues.len()));
        output.push_str(&format!(
            "**Critical:** {}, **High:** {}, **Medium:** {}, **Low:** {}\n\n",
            report
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Critical)
                .count(),
            report
                .issues
                .iter()
                .filter(|i| i.severity == Severity::High)
                .count(),
            report
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Medium)
                .count(),
            report
                .issues
                .iter()
                .filter(|i| i.severity == Severity::Low)
                .count(),
        ));

        // 按严重程度分组显示问题
        for severity in [
            Severity::Critical,
            Severity::High,
            Severity::Medium,
            Severity::Low,
        ] {
            let issues: Vec<_> = report
                .issues
                .iter()
                .filter(|i| i.severity == severity)
                .collect();

            if !issues.is_empty() {
                output.push_str(&format!("## {} Issues\n\n", severity.label()));

                for issue in issues {
                    output.push_str(&format!("### {}\n\n", issue.title));

                    if let Some(location) = &issue.location {
                        output.push_str(&format!("- **Location:** `{}`\n", location));
                    }

                    output.push_str(&format!("- **Problem:** {}\n", issue.description));
                    output.push_str(&format!("- **Impact:** {}\n", issue.impact));
                    output.push_str(&format!(
                        "- **Recommendation:** {}\n\n",
                        issue.recommendation
                    ));
                }
            }
        }

        if !report.positive_findings.is_empty() {
            output.push_str("## Positive Findings\n\n");
            for finding in &report.positive_findings {
                output.push_str(&format!("- {}\n", finding));
            }
            output.push('\n');
        }

        if !report.recommendations.is_empty() {
            output.push_str("## Recommendations\n\n");
            for rec in &report.recommendations {
                output.push_str(&format!("- {}\n", rec));
            }
            output.push('\n');
        }

        // 总体结论
        output.push_str("## Conclusion\n\n");
        match report.overall_status {
            ScrutinyStatus::Passed => {
                output.push_str("Code quality meets project standards. Ready to proceed.\n");
            }
            ScrutinyStatus::Failed => {
                output.push_str("Code quality is below acceptable standards. Critical or high issues must be addressed before proceeding.\n");
            }
            ScrutinyStatus::Partial => {
                output.push_str("Code is acceptable but has clear improvement opportunities. Consider addressing high and medium issues.\n");
            }
        }

        output
    }
}

impl Validator for ScrutinyValidator {
    type Report = ScrutinyReport;

    fn validate(&self, handoff: &Handoff) -> Self::Report {
        let mut issues = Vec::new();
        let mut positive_findings = Vec::new();
        let mut recommendations = Vec::new();

        // 检查 Handoff 的代码审查结果
        let code_review = &handoff.verification.code_review;

        // 根据代码审查状态生成问题
        match code_review.status {
            ReviewStatus::Passed => {
                positive_findings.push(format!(
                    "Code review passed with {} issues fixed",
                    code_review.issues_fixed
                ));
            }
            ReviewStatus::Failed => {
                issues.push(Issue {
                    title: "Code review failed".to_string(),
                    severity: Severity::Critical,
                    location: None,
                    description: "Code review indicated failure".to_string(),
                    impact: "Code may have critical quality issues".to_string(),
                    recommendation: "Review and address all code review findings".to_string(),
                });
            }
            ReviewStatus::Partial => {
                issues.push(Issue {
                    title: "Code review partially completed".to_string(),
                    severity: Severity::Medium,
                    location: None,
                    description: "Code review was partially completed".to_string(),
                    impact: "Some issues may remain unaddressed".to_string(),
                    recommendation: "Complete code review and address findings".to_string(),
                });
            }
            ReviewStatus::Skipped => {
                issues.push(Issue {
                    title: "Code review was skipped".to_string(),
                    severity: Severity::High,
                    location: None,
                    description: "No code review was performed".to_string(),
                    impact: "Code quality has not been verified".to_string(),
                    recommendation: "Perform thorough code review".to_string(),
                });
            }
        }

        // 检查文件变更
        if handoff.files_modified.is_empty() && handoff.files_created.is_empty() {
            positive_findings
                .push("No file changes reported (configuration or documentation only)".to_string());
        } else {
            // 检查是否有大量文件变更（可能需要重构）
            let total_changes = handoff.files_modified.len() + handoff.files_created.len();
            if total_changes > 10 {
                issues.push(Issue {
                    title: "Large number of file changes".to_string(),
                    severity: Severity::Medium,
                    location: None,
                    description: format!("{} files were changed", total_changes),
                    impact: "May indicate need for refactoring or better separation of concerns"
                        .to_string(),
                    recommendation: "Consider if changes can be split into smaller, focused PRs"
                        .to_string(),
                });
            } else {
                positive_findings.push(format!(
                    "Reasonable number of file changes: {}",
                    total_changes
                ));
            }
        }

        // 检查阻塞问题
        if !handoff.blockers.is_empty() {
            for blocker in &handoff.blockers {
                issues.push(Issue {
                    title: "Blocker reported".to_string(),
                    severity: Severity::Critical,
                    location: None,
                    description: blocker.clone(),
                    impact: "Blocks progress on the mission".to_string(),
                    recommendation: "Address blockers before proceeding".to_string(),
                });
            }
        }

        // 检查摘要质量
        if handoff.salient_summary.is_empty() {
            issues.push(Issue {
                title: "Empty summary".to_string(),
                severity: Severity::High,
                location: None,
                description: "Handoff summary is empty".to_string(),
                impact: "Difficult to understand what was accomplished".to_string(),
                recommendation: "Provide a clear 1-2 sentence summary".to_string(),
            });
        } else if handoff.salient_summary.len() < 20 {
            issues.push(Issue {
                title: "Summary too brief".to_string(),
                severity: Severity::Low,
                location: None,
                description: format!(
                    "Summary is only {} characters",
                    handoff.salient_summary.len()
                ),
                impact: "May not capture key outcomes".to_string(),
                recommendation: "Expand summary to better describe work done".to_string(),
            });
        } else {
            positive_findings.push("Clear and comprehensive summary provided".to_string());
        }

        // 检查实现内容
        if handoff.what_was_implemented.is_empty() {
            issues.push(Issue {
                title: "No implementation details".to_string(),
                severity: Severity::High,
                location: None,
                description: "What was implemented list is empty".to_string(),
                impact: "Unclear what work was completed".to_string(),
                recommendation: "List all key changes and features implemented".to_string(),
            });
        }

        // 生成总体建议
        if issues.is_empty() {
            recommendations.push("No issues found. Code quality is good.".to_string());
        } else {
            let critical_count = issues
                .iter()
                .filter(|i| i.severity == Severity::Critical)
                .count();
            let high_count = issues
                .iter()
                .filter(|i| i.severity == Severity::High)
                .count();

            if critical_count > 0 {
                recommendations.push(format!(
                    "Address {} critical issue(s) before proceeding",
                    critical_count
                ));
            }
            if high_count > 0 {
                recommendations.push(format!(
                    "Address {} high-priority issue(s) as soon as possible",
                    high_count
                ));
            }
        }

        // 确定总体状态
        let overall_status = if self.config.strict {
            if issues.iter().any(|i| i.severity == Severity::Critical) {
                ScrutinyStatus::Failed
            } else if !issues.is_empty() {
                ScrutinyStatus::Partial
            } else {
                ScrutinyStatus::Passed
            }
        } else {
            if issues.iter().any(|i| i.severity == Severity::Critical)
                || issues
                    .iter()
                    .filter(|i| i.severity == Severity::High)
                    .count()
                    > 2
            {
                ScrutinyStatus::Failed
            } else if !issues.is_empty() {
                ScrutinyStatus::Partial
            } else {
                ScrutinyStatus::Passed
            }
        };

        ScrutinyReport {
            overall_status,
            issues,
            positive_findings,
            recommendations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_passed_handoff() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Successfully implemented feature A with proper error handling and tests")
            .add_implementation("Feature A implementation")
            .add_implementation("Unit tests for feature A")
            .with_next_steps("Proceed to feature B");

        let validator = ScrutinyValidator::with_defaults();
        let report = validator.validate(&handoff);

        assert_eq!(report.overall_status, ScrutinyStatus::Passed);
        assert!(report.issues.is_empty());
        assert!(!report.positive_findings.is_empty());
    }

    #[test]
    fn validate_handoff_with_blockers() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Implementation complete")
            .add_implementation("Feature A")
            .with_next_steps("Testing")
            .add_blocker("Waiting for API dependency");

        let validator = ScrutinyValidator::with_defaults();
        let report = validator.validate(&handoff);

        assert_eq!(report.overall_status, ScrutinyStatus::Failed);
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.severity == Severity::Critical)
        );
    }

    #[test]
    fn validate_empty_handoff() {
        let handoff = Handoff::new("test-worker".to_string());

        let validator = ScrutinyValidator::with_defaults();
        let report = validator.validate(&handoff);

        assert!(!matches!(report.overall_status, ScrutinyStatus::Passed));
        assert!(!report.issues.is_empty());
    }

    #[test]
    fn report_as_markdown() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Test completed")
            .add_implementation("Feature A")
            .with_next_steps("Next steps");

        let validator = ScrutinyValidator::with_defaults();
        let report = validator.validate(&handoff);
        let markdown = validator.report_as_markdown(&report);

        assert!(markdown.contains("# Scrutiny Validation Report"));
        assert!(markdown.contains("**Overall Status:**"));
        assert!(markdown.contains("**Issues Found:**"));
    }
}

//! Scrutiny Validator - 代码审查验证器。
//!
//! 通过静态分析验证代码质量。

use serde::Deserialize;
use serde::Serialize;

use crate::handoff::Handoff;
use crate::handoff::ReviewStatus;
use crate::validators::Severity;
use crate::validators::Validator;
use crate::validators::ValidatorConfig;

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
    Passed,
    Failed,
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
    pub fn new(config: ValidatorConfig) -> Self {
        Self { config }
    }

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

        output
    }
}

impl Validator for ScrutinyValidator {
    type Report = ScrutinyReport;

    fn validate(&self, handoff: &Handoff) -> ScrutinyReport {
        let mut issues = Vec::new();
        let mut positive_findings = Vec::new();
        let mut recommendations = Vec::new();

        // 检查是否有阻塞问题
        if !handoff.blockers.is_empty() {
            issues.push(Issue {
                title: "Blockers present".to_string(),
                severity: Severity::Critical,
                location: None,
                description: format!(
                    "{} blocker(s) found: {}",
                    handoff.blockers.len(),
                    handoff.blockers.join(", ")
                ),
                impact: "Mission cannot proceed until blockers are resolved".to_string(),
                recommendation: "Resolve all blockers before proceeding".to_string(),
            });
        }

        // 检查验证结果
        if handoff.verification.code_review.status == ReviewStatus::Passed {
            positive_findings.push("Code review passed".to_string());
        } else if handoff.verification.code_review.status == ReviewStatus::Failed {
            issues.push(Issue {
                title: "Code review failed".to_string(),
                severity: Severity::High,
                location: None,
                description: handoff.verification.code_review.findings.clone(),
                impact: "Code quality issues may affect reliability".to_string(),
                recommendation: "Address code review findings before proceeding".to_string(),
            });
        }

        if handoff.verification.code_review.issues_found > 0
            && handoff.verification.code_review.issues_fixed
                < handoff.verification.code_review.issues_found
        {
            let unfixed = handoff.verification.code_review.issues_found
                - handoff.verification.code_review.issues_fixed;
            issues.push(Issue {
                title: "Unfixed code review issues".to_string(),
                severity: Severity::Medium,
                location: None,
                description: format!(
                    "{unfixed} issue(s) from code review remain unfixed out of {} total",
                    handoff.verification.code_review.issues_found
                ),
                impact: "Remaining issues may cause problems in later stages".to_string(),
                recommendation: "Fix remaining code review issues or document exceptions"
                    .to_string(),
            });
        }

        // 检查摘要质量
        if handoff.salient_summary.len() > 20 {
            positive_findings.push("Concise summary provided".to_string());
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
                    "Address {critical_count} critical issue(s) before proceeding"
                ));
            }
            if high_count > 0 {
                recommendations.push(format!(
                    "Address {high_count} high-priority issue(s) as soon as possible"
                ));
            }
        }

        let overall_status = if self.config.strict {
            if issues.iter().any(|i| i.severity == Severity::Critical) {
                ScrutinyStatus::Failed
            } else if !issues.is_empty() {
                ScrutinyStatus::Partial
            } else {
                ScrutinyStatus::Passed
            }
        } else if issues.iter().any(|i| i.severity == Severity::Critical)
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

        let mut verification = crate::handoff::Verification::default();
        verification.code_review.status = ReviewStatus::Passed;
        let handoff = handoff.with_verification(verification);

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

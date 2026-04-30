//! User Testing Validator - 用户测试验证器。
//!
//! 验证功能从用户视角的正确性和可用性。

use serde::Deserialize;
use serde::Serialize;

use crate::handoff::Handoff;
use crate::handoff::ReviewStatus;
use crate::validators::Severity;
use crate::validators::Validator;
use crate::validators::ValidatorConfig;

/// User Testing 验证报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserTestingReport {
    /// 总体状态。
    pub overall_status: TestingStatus,
    /// 测试用例结果。
    pub test_results: Vec<TestResult>,
    /// 发现的问题列表。
    pub issues: Vec<TestIssue>,
    /// 正面发现列表。
    pub positive_findings: Vec<String>,
    /// 总体建议。
    pub recommendations: Vec<String>,
}

/// User Testing 总体状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestingStatus {
    Passed,
    Failed,
    Partial,
}

impl TestingStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Failed => "FAILED",
            Self::Partial => "PARTIAL",
        }
    }
}

/// 测试分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestCategory {
    Smoke,
    Normal,
    Edge,
    Error,
    Integration,
}

impl TestCategory {
    pub fn label(self) -> &'static str {
        match self {
            Self::Smoke => "Smoke",
            Self::Normal => "Normal",
            Self::Edge => "Edge",
            Self::Error => "Error",
            Self::Integration => "Integration",
        }
    }
}

/// 单个测试用例结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResult {
    pub name: String,
    pub category: TestCategory,
    pub status: TestStatus,
    pub notes: String,
}

/// 测试状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
}

impl TestStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "PASS",
            Self::Failed => "FAIL",
            Self::Skipped => "SKIP",
        }
    }
}

/// 测试发现的问题。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestIssue {
    pub title: String,
    pub test_case: String,
    pub category: TestCategory,
    pub severity: Severity,
    pub expected: String,
    pub actual: String,
    pub impact: String,
    pub recommendation: String,
}

/// User Testing 验证器。
#[derive(Debug, Clone)]
pub struct UserTestingValidator {
    config: ValidatorConfig,
}

impl UserTestingValidator {
    pub fn new(config: ValidatorConfig) -> Self {
        Self { config }
    }

    pub fn with_defaults() -> Self {
        Self::new(ValidatorConfig::default())
    }

    /// 生成 Markdown 格式报告。
    pub fn report_as_markdown(&self, report: &UserTestingReport) -> String {
        let mut output = String::new();

        output.push_str("# User Testing Validation Report\n\n");
        output.push_str(&format!(
            "**Overall Status:** {}\n",
            report.overall_status.label()
        ));

        let total_tests = report.test_results.len();
        let passed = report
            .test_results
            .iter()
            .filter(|t| t.status == TestStatus::Passed)
            .count();
        let failed = report
            .test_results
            .iter()
            .filter(|t| t.status == TestStatus::Failed)
            .count();
        let skipped = report
            .test_results
            .iter()
            .filter(|t| t.status == TestStatus::Skipped)
            .count();

        output.push_str(&format!(
            "**Tests Executed:** {} (Passed: {}, Failed: {}, Skipped: {})\n\n",
            total_tests, passed, failed, skipped
        ));

        if !report.test_results.is_empty() {
            output.push_str("## Test Results\n\n");
            output.push_str("| Test | Category | Status | Notes |\n");
            output.push_str("|------|----------|--------|-------|\n");
            for test in &report.test_results {
                output.push_str(&format!(
                    "| {} | {} | {} | {} |\n",
                    test.name,
                    test.category.label(),
                    test.status.label(),
                    test.notes
                ));
            }
            output.push('\n');
        }

        if !report.issues.is_empty() {
            output.push_str("## Issues\n\n");
            for issue in &report.issues {
                output.push_str(&format!(
                    "### {} [{}]\n\n",
                    issue.title,
                    issue.severity.label()
                ));
                output.push_str(&format!("- **Test Case:** {}\n", issue.test_case));
                output.push_str(&format!(
                    "- **Expected:** {}\n- **Actual:** {}\n",
                    issue.expected, issue.actual
                ));
                output.push_str(&format!("- **Impact:** {}\n", issue.impact));
                output.push_str(&format!(
                    "- **Recommendation:** {}\n\n",
                    issue.recommendation
                ));
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

impl Validator for UserTestingValidator {
    type Report = UserTestingReport;

    fn validate(&self, handoff: &Handoff) -> UserTestingReport {
        let mut test_results = Vec::new();
        let mut issues = Vec::new();
        let mut positive_findings = Vec::new();
        let mut recommendations = Vec::new();

        let user_testing = &handoff.verification.user_testing;

        // 根据测试结果生成测试用例
        if user_testing.status == ReviewStatus::Passed {
            test_results.push(TestResult {
                name: "core_functionality".to_string(),
                category: TestCategory::Smoke,
                status: TestStatus::Passed,
                notes: user_testing.results.clone(),
            });
            positive_findings.push(format!(
                "All {} tests passed",
                user_testing.test_cases_executed
            ));
        } else if user_testing.status == ReviewStatus::Failed {
            let failed_count = user_testing.test_cases_executed - user_testing.test_cases_passed;
            test_results.push(TestResult {
                name: "core_functionality".to_string(),
                category: TestCategory::Smoke,
                status: TestStatus::Failed,
                notes: format!("{failed_count} test(s) failed: {}", user_testing.results),
            });

            let severity = if (user_testing.test_cases_passed as f64
                / user_testing.test_cases_executed.max(1) as f64)
                <= 0.5
            {
                Severity::Critical
            } else {
                Severity::High
            };

            issues.push(TestIssue {
                title: "User tests failed".to_string(),
                test_case: "core_functionality".to_string(),
                category: TestCategory::Smoke,
                severity,
                expected: "All tests should pass".to_string(),
                actual: format!(
                    "{}/{} tests passed",
                    user_testing.test_cases_passed, user_testing.test_cases_executed
                ),
                impact: "Failed tests indicate broken user-facing functionality".to_string(),
                recommendation: "Fix failing tests before proceeding".to_string(),
            });
        } else if user_testing.status == ReviewStatus::Skipped {
            // 如果有文件变更但没有测试，标记为需要测试
            if !handoff.files_created.is_empty() || !handoff.files_modified.is_empty() {
                test_results.push(TestResult {
                    name: "smoke_test".to_string(),
                    category: TestCategory::Smoke,
                    status: TestStatus::Skipped,
                    notes: "User testing was skipped but files were changed".to_string(),
                });

                issues.push(TestIssue {
                    title: "User testing skipped".to_string(),
                    test_case: "smoke_test".to_string(),
                    category: TestCategory::Smoke,
                    severity: Severity::High,
                    expected: "User testing should be performed for changed files".to_string(),
                    actual: "User testing was skipped".to_string(),
                    impact: "Changed files may have user-facing regressions".to_string(),
                    recommendation: "Run user tests on all changed files".to_string(),
                });
            }
        }

        // 检查通过率
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
                "Address {critical_count} critical issue(s) immediately"
            ));
        }
        if high_count > 0 {
            recommendations.push(format!(
                "Address {high_count} high-priority issue(s) as soon as possible"
            ));
        }
        if issues.is_empty() && !test_results.is_empty() {
            recommendations.push("All tests passed. User experience is good.".to_string());
        }

        let overall_status = if self.config.strict {
            if test_results
                .iter()
                .any(|t| t.category == TestCategory::Smoke && t.status == TestStatus::Failed)
            {
                TestingStatus::Failed
            } else if !issues.is_empty() {
                TestingStatus::Partial
            } else {
                TestingStatus::Passed
            }
        } else {
            let smoke_failed = test_results
                .iter()
                .any(|t| t.category == TestCategory::Smoke && t.status == TestStatus::Failed);
            let pass_rate = if user_testing.test_cases_executed > 0 {
                (user_testing.test_cases_passed as f64 / user_testing.test_cases_executed as f64)
                    * 100.0
            } else {
                0.0
            };

            if smoke_failed || critical_count > 0 {
                TestingStatus::Failed
            } else if pass_rate >= 90.0 && high_count == 0 {
                TestingStatus::Passed
            } else {
                TestingStatus::Partial
            }
        };

        UserTestingReport {
            overall_status,
            test_results,
            issues,
            positive_findings,
            recommendations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handoff::UserTestingResult;

    #[test]
    fn validate_passed_user_tests() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Feature implemented and tested")
            .add_implementation("Feature A")
            .with_next_steps("Next phase");

        let mut verification = crate::handoff::Verification::default();
        verification.user_testing = UserTestingResult {
            status: ReviewStatus::Passed,
            results: "All tests passed".to_string(),
            test_cases_executed: 10,
            test_cases_passed: 10,
        };
        let handoff = handoff.with_verification(verification);

        let validator = UserTestingValidator::with_defaults();
        let report = validator.validate(&handoff);

        assert_eq!(report.overall_status, TestingStatus::Passed);
        assert!(report.issues.is_empty());
        assert!(!report.positive_findings.is_empty());
    }

    #[test]
    fn validate_failed_user_tests() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Feature implemented")
            .add_implementation("Feature A")
            .with_next_steps("Testing");

        let mut verification = crate::handoff::Verification::default();
        verification.user_testing = UserTestingResult {
            status: ReviewStatus::Failed,
            results: "Critical tests failed".to_string(),
            test_cases_executed: 10,
            test_cases_passed: 5,
        };
        let handoff = handoff.with_verification(verification);

        let validator = UserTestingValidator::with_defaults();
        let report = validator.validate(&handoff);

        assert_eq!(report.overall_status, TestingStatus::Failed);
        assert!(!report.issues.is_empty());
        assert!(
            report
                .issues
                .iter()
                .any(|i| i.severity == Severity::Critical)
        );
    }

    #[test]
    fn validate_skipped_user_tests() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Feature implemented")
            .add_implementation("Feature A")
            .add_file_creation("src/new_feature.rs", "New feature")
            .with_next_steps("Testing");

        let validator = UserTestingValidator::with_defaults();
        let report = validator.validate(&handoff);

        assert!(!matches!(report.overall_status, TestingStatus::Passed));
        assert!(
            report
                .issues
                .iter()
                .any(|i| matches!(i.category, TestCategory::Smoke))
        );
    }

    #[test]
    fn report_as_markdown() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Feature implemented")
            .add_implementation("Feature A")
            .with_next_steps("Testing");

        let validator = UserTestingValidator::with_defaults();
        let report = validator.validate(&handoff);
        let markdown = validator.report_as_markdown(&report);

        assert!(markdown.contains("# User Testing Validation Report"));
        assert!(markdown.contains("**Overall Status:**"));
        assert!(markdown.contains("**Tests Executed:**"));
    }
}

//! User Testing Validator - 用户测试验证器。
//!
//! 验证功能从用户视角的正确性和可用性。

use crate::mission::handoff::{Handoff, ReviewStatus, UserTestingResult};
use crate::mission::validators::{Validator, ValidatorConfig};
use serde::{Deserialize, Serialize};

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
    /// 通过。
    Passed,
    /// 失败。
    Failed,
    /// 部分通过。
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
    /// 冒烟测试。
    Smoke,
    /// 正常用例。
    Normal,
    /// 边界用例。
    Edge,
    /// 错误用例。
    Error,
    /// 集成用例。
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
    /// 测试名称。
    pub name: String,
    /// 测试分类。
    pub category: TestCategory,
    /// 测试状态。
    pub status: TestStatus,
    /// 备注。
    pub notes: String,
}

/// 测试状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    /// 通过。
    Passed,
    /// 失败。
    Failed,
    /// 跳过。
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
    /// 问题标题。
    pub title: String,
    /// 测试用例。
    pub test_case: String,
    /// 分类。
    pub category: TestCategory,
    /// 严重程度。
    pub severity: IssueSeverity,
    /// 预期行为。
    pub expected: String,
    /// 实际行为。
    pub actual: String,
    /// 影响。
    pub impact: String,
    /// 建议。
    pub recommendation: String,
}

/// 问题严重程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    /// 关键问题。
    Critical,
    /// 高优先级问题。
    High,
    /// 中等优先级问题。
    Medium,
    /// 低优先级问题。
    Low,
}

impl IssueSeverity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Critical => "Critical",
            Self::High => "High",
            Self::Medium => "Medium",
            Self::Low => "Low",
        }
    }
}

/// User Testing 验证器。
#[derive(Debug, Clone)]
pub struct UserTestingValidator {
    config: ValidatorConfig,
}

impl UserTestingValidator {
    /// 创建新的 User Testing 验证器。
    pub fn new(config: ValidatorConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建验证器。
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
        let passed_tests = report.test_results.iter()
            .filter(|t| t.status == TestStatus::Passed)
            .count();
        let failed_tests = report.test_results.iter()
            .filter(|t| t.status == TestStatus::Failed)
            .count();
        let skipped_tests = report.test_results.iter()
            .filter(|t| t.status == TestStatus::Skipped)
            .count();

        output.push_str(&format!("**Tests Executed:** {}\n", total_tests));
        output.push_str(&format!("**Tests Passed:** {}\n", passed_tests));
        output.push_str(&format!("**Tests Failed:** {}\n", failed_tests));
        output.push_str(&format!("**Tests Skipped:** {}\n\n", skipped_tests));

        // 测试结果表格
        if !report.test_results.is_empty() {
            output.push_str("## Test Results\n\n");

            // 按分类组织
            for category in [
                TestCategory::Smoke,
                TestCategory::Normal,
                TestCategory::Edge,
                TestCategory::Error,
                TestCategory::Integration,
            ] {
                let tests: Vec<_> = report.test_results.iter()
                    .filter(|t| t.category == category)
                    .collect();

                if !tests.is_empty() {
                    output.push_str(&format!("### {} Tests\n\n", category.label()));
                    output.push_str("| Test | Status | Notes |\n");
                    output.push_str("|------|--------|-------|\n");

                    for test in tests {
                        output.push_str(&format!(
                            "| {} | {} | {} |\n",
                            test.name,
                            test.status.label(),
                            test.notes
                        ));
                    }
                    output.push('\n');
                }
            }
        }

        // 发现的问题
        if !report.issues.is_empty() {
            output.push_str("## Issues Found\n\n");

            for severity in [IssueSeverity::Critical, IssueSeverity::High, IssueSeverity::Medium, IssueSeverity::Low] {
                let issues: Vec<_> = report.issues.iter()
                    .filter(|i| i.severity == severity)
                    .collect();

                if !issues.is_empty() {
                    output.push_str(&format!("### {} Issues\n\n", severity.label()));

                    for issue in issues {
                        output.push_str(&format!("#### {}\n\n", issue.title));
                        output.push_str(&format!("- **Test Case:** {}\n", issue.test_case));
                        output.push_str(&format!("- **Category:** {}\n", issue.category.label()));
                        output.push_str(&format!("- **Expected:** {}\n", issue.expected));
                        output.push_str(&format!("- **Actual:** {}\n", issue.actual));
                        output.push_str(&format!("- **Impact:** {}\n", issue.impact));
                        output.push_str(&format!("- **Recommendation:** {}\n\n", issue.recommendation));
                    }
                }
            }
        }

        // 正面发现
        if !report.positive_findings.is_empty() {
            output.push_str("## Positive Findings\n\n");
            for finding in &report.positive_findings {
                output.push_str(&format!("- {}\n", finding));
            }
            output.push('\n');
        }

        // 建议
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
            TestingStatus::Passed => {
                output.push_str("All critical tests passed. User experience is acceptable.\n");
            }
            TestingStatus::Failed => {
                output.push_str("Critical tests failed or smoke tests failed. User experience is not acceptable.\n");
            }
            TestingStatus::Partial => {
                output.push_str("Some tests failed but core functionality works. User experience is acceptable with some friction.\n");
            }
        }

        output
    }
}

impl Validator for UserTestingValidator {
    type Report = UserTestingReport;

    fn validate(&self, handoff: &Handoff) -> Self::Report {
        let mut test_results = Vec::new();
        let mut issues = Vec::new();
        let mut positive_findings = Vec::new();
        let mut recommendations = Vec::new();

        // 从 Handoff 提取用户测试结果
        let user_testing = &handoff.verification.user_testing;

        // 检查测试状态
        match user_testing.status {
            ReviewStatus::Passed => {
                // 添加综合测试结果
                test_results.push(TestResult {
                    name: "Overall User Testing".to_string(),
                    category: TestCategory::Normal,
                    status: TestStatus::Passed,
                    notes: format!(
                        "{} of {} test cases passed",
                        user_testing.test_cases_passed, user_testing.test_cases_executed
                    ),
                });

                positive_findings.push(format!(
                    "All user tests passed ({} test cases)",
                    user_testing.test_cases_executed
                ));
            }
            ReviewStatus::Failed => {
                test_results.push(TestResult {
                    name: "Overall User Testing".to_string(),
                    category: TestCategory::Smoke,
                    status: TestStatus::Failed,
                    notes: user_testing.results.clone(),
                });

                issues.push(TestIssue {
                    title: "User testing failed".to_string(),
                    test_case: "Overall Testing".to_string(),
                    category: TestCategory::Smoke,
                    severity: IssueSeverity::Critical,
                    expected: "All tests should pass".to_string(),
                    actual: format!("Tests failed: {}", user_testing.results),
                    impact: "Core functionality may not work for users".to_string(),
                    recommendation: "Review and fix failing tests before proceeding".to_string(),
                });
            }
            ReviewStatus::Partial => {
                let pass_rate = if user_testing.test_cases_executed > 0 {
                    (user_testing.test_cases_passed as f64 / user_testing.test_cases_executed as f64) * 100.0
                } else {
                    0.0
                };

                test_results.push(TestResult {
                    name: "Overall User Testing".to_string(),
                    category: TestCategory::Normal,
                    status: TestStatus::Passed,
                    notes: format!(
                        "{} of {} tests passed ({:.0}%)",
                        user_testing.test_cases_passed,
                        user_testing.test_cases_executed,
                        pass_rate
                    ),
                });

                if pass_rate >= 90.0 {
                    positive_findings.push(format!(
                        "High test pass rate: {:.0}%",
                        pass_rate
                    ));
                } else if pass_rate >= 70.0 {
                    issues.push(TestIssue {
                        title: "Some tests failed".to_string(),
                        test_case: "Overall Testing".to_string(),
                        category: TestCategory::Normal,
                        severity: IssueSeverity::Medium,
                        expected: "All tests should pass".to_string(),
                        actual: format!("{:.0}% of tests passed", pass_rate),
                        impact: "Some features may not work correctly".to_string(),
                        recommendation: "Fix failing tests to improve reliability".to_string(),
                    });
                } else {
                    issues.push(TestIssue {
                        title: "Many tests failed".to_string(),
                        test_case: "Overall Testing".to_string(),
                        category: TestCategory::Smoke,
                        severity: IssueSeverity::High,
                        expected: "All tests should pass".to_string(),
                        actual: format!("Only {:.0}% of tests passed", pass_rate),
                        impact: "Significant functionality may be broken".to_string(),
                        recommendation: "Critical: Fix failing tests before proceeding".to_string(),
                    });
                }
            }
            ReviewStatus::Skipped => {
                test_results.push(TestResult {
                    name: "User Testing".to_string(),
                    category: TestCategory::Smoke,
                    status: TestStatus::Skipped,
                    notes: "Testing was skipped".to_string(),
                });

                issues.push(TestIssue {
                    title: "User testing was skipped".to_string(),
                    test_case: "Overall Testing".to_string(),
                    category: TestCategory::Smoke,
                    severity: IssueSeverity::High,
                    expected: "Tests should be run".to_string(),
                    actual: "No testing was performed".to_string(),
                    impact: "Cannot verify functionality works for users".to_string(),
                    recommendation: "Run user tests before proceeding".to_string(),
                });
            }
        }

        // 检查是否有文件变更（假设有变更就需要测试）
        if !handoff.files_modified.is_empty() || !handoff.files_created.is_empty() {
            if user_testing.test_cases_executed == 0 {
                issues.push(TestIssue {
                    title: "No tests executed despite code changes".to_string(),
                    test_case: "Test Coverage".to_string(),
                    category: TestCategory::Normal,
                    severity: IssueSeverity::High,
                    expected: "Tests should be run for code changes".to_string(),
                    actual: "No tests were executed".to_string(),
                    impact: "Cannot verify code changes work correctly".to_string(),
                    recommendation: "Write and run tests for new functionality".to_string(),
                });
            }
        }

        // 检查阻塞问题对用户体验的影响
        if !handoff.blockers.is_empty() {
            for blocker in &handoff.blockers {
                issues.push(TestIssue {
                    title: "User-facing blocker".to_string(),
                    test_case: "Workflow Test".to_string(),
                    category: TestCategory::Smoke,
                    severity: IssueSeverity::Critical,
                    expected: "User can complete their workflow".to_string(),
                    actual: format!("Blocked by: {}", blocker),
                    impact: "Users cannot complete their tasks".to_string(),
                    recommendation: "Resolve blockers before release".to_string(),
                });
            }
        }

        // 检查剩余工作对用户体验的影响
        if !handoff.verification.remaining_work.is_empty() {
            test_results.push(TestResult {
                name: "Remaining Work Assessment".to_string(),
                category: TestCategory::Normal,
                status: TestStatus::Skipped,
                notes: handoff.verification.remaining_work.clone(),
            });

            recommendations.push(format!(
                "Address remaining work: {}",
                handoff.verification.remaining_work
            ));
        }

        // 生成总体建议
        let critical_count = issues.iter().filter(|i| i.severity == IssueSeverity::Critical).count();
        let high_count = issues.iter().filter(|i| i.severity == IssueSeverity::High).count();

        if critical_count > 0 {
            recommendations.push(format!(
                "Critical: Address {} critical issue(s) immediately",
                critical_count
            ));
        }
        if high_count > 0 {
            recommendations.push(format!(
                "Address {} high-priority issue(s) as soon as possible",
                high_count
            ));
        }

        if issues.is_empty() && !test_results.is_empty() {
            recommendations.push("All tests passed. User experience is good.".to_string());
        }

        // 确定总体状态
        let overall_status = if self.config.strict {
            if test_results.iter().any(|t| t.category == TestCategory::Smoke && t.status == TestStatus::Failed) {
                TestingStatus::Failed
            } else if !issues.is_empty() {
                TestingStatus::Partial
            } else {
                TestingStatus::Passed
            }
        } else {
            let smoke_failed = test_results.iter()
                .any(|t| t.category == TestCategory::Smoke && t.status == TestStatus::Failed);

            let pass_rate = if user_testing.test_cases_executed > 0 {
                (user_testing.test_cases_passed as f64 / user_testing.test_cases_executed as f64) * 100.0
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

    #[test]
    fn validate_passed_user_tests() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Feature implemented and tested")
            .add_implementation("Feature A")
            .with_next_steps("Next phase");

        // 模拟通过的用户测试结果
        let mut verification = crate::mission::handoff::Verification::default();
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

        // 模拟失败的用户测试结果
        let mut verification = crate::mission::handoff::Verification::default();
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
        assert!(report.issues.iter().any(|i| i.severity == IssueSeverity::Critical));
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
        assert!(report.issues.iter().any(|i| matches!(i.category, TestCategory::Smoke)));
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

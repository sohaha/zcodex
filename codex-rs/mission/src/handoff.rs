//! Mission Handoff（交接）机制。
//!
//! 定义 Worker 之间交接的标准格式和相关操作。

use crate::MissionResult;
use crate::error::MissionError;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

/// Handoff 文件扩展名。
pub const HANDOFF_EXT: &str = "json";

/// Worker 生成的标准化 Handoff JSON。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Handoff {
    /// Worker 名称。
    pub worker: String,
    /// 时间戳（ISO-8601 格式）。
    pub timestamp: String,
    /// 核心摘要（1-2 句话）。
    pub salient_summary: String,
    /// 实现内容列表。
    pub what_was_implemented: Vec<String>,
    /// 修改的文件列表。
    #[serde(default)]
    pub files_modified: Vec<FileChange>,
    /// 创建的文件列表。
    #[serde(default)]
    pub files_created: Vec<FileCreation>,
    /// 验证结果。
    pub verification: Verification,
    /// 下一步建议。
    pub next_steps: String,
    /// 阻塞问题列表。
    #[serde(default)]
    pub blockers: Vec<String>,
}

/// 文件变更记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// 文件路径（相对于工作区根目录）。
    pub path: String,
    /// 变更摘要。
    pub change_summary: String,
}

/// 文件创建记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileCreation {
    /// 文件路径（相对于工作区根目录）。
    pub path: String,
    /// 文件用途。
    pub purpose: String,
}

/// 验证结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verification {
    /// 代码审查结果。
    pub code_review: CodeReviewResult,
    /// 用户测试结果。
    pub user_testing: UserTestingResult,
    /// 剩余工作描述。
    pub remaining_work: String,
}

/// 代码审查结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeReviewResult {
    /// 审查状态。
    pub status: ReviewStatus,
    /// 审查发现。
    pub findings: String,
    /// 发现的问题数量。
    pub issues_found: u32,
    /// 修复的问题数量。
    pub issues_fixed: u32,
}

/// 用户测试结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserTestingResult {
    /// 测试状态。
    pub status: ReviewStatus,
    /// 测试结果。
    pub results: String,
    /// 执行的测试用例数。
    pub test_cases_executed: u32,
    /// 通过的测试用例数。
    pub test_cases_passed: u32,
}

/// 审查/测试状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// 通过。
    Passed,
    /// 失败。
    Failed,
    /// 部分完成。
    Partial,
    /// 跳过。
    Skipped,
}

impl ReviewStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Partial => "partial",
            Self::Skipped => "skipped",
        }
    }
}

impl Handoff {
    /// 创建新的 Handoff。
    pub fn new(worker: String) -> Self {
        Self {
            worker,
            timestamp: chrono::Utc::now().to_rfc3339(),
            salient_summary: String::new(),
            what_was_implemented: Vec::new(),
            files_modified: Vec::new(),
            files_created: Vec::new(),
            verification: Verification::default(),
            next_steps: String::new(),
            blockers: Vec::new(),
        }
    }

    /// 设置核心摘要。
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.salient_summary = summary.into();
        self
    }

    /// 添加实现内容。
    pub fn add_implementation(mut self, item: impl Into<String>) -> Self {
        self.what_was_implemented.push(item.into());
        self
    }

    /// 添加文件变更。
    pub fn add_file_change(mut self, path: impl Into<String>, summary: impl Into<String>) -> Self {
        self.files_modified.push(FileChange {
            path: path.into(),
            change_summary: summary.into(),
        });
        self
    }

    /// 添加文件创建。
    pub fn add_file_creation(
        mut self,
        path: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Self {
        self.files_created.push(FileCreation {
            path: path.into(),
            purpose: purpose.into(),
        });
        self
    }

    /// 设置验证结果。
    pub fn with_verification(mut self, verification: Verification) -> Self {
        self.verification = verification;
        self
    }

    /// 设置下一步建议。
    pub fn with_next_steps(mut self, steps: impl Into<String>) -> Self {
        self.next_steps = steps.into();
        self
    }

    /// 添加阻塞问题。
    pub fn add_blocker(mut self, blocker: impl Into<String>) -> Self {
        self.blockers.push(blocker.into());
        self
    }

    /// 验证 Handoff 内容的完整性。
    pub fn validate(&self) -> Result<(), HandoffValidationError> {
        if self.salient_summary.is_empty() {
            return Err(HandoffValidationError::EmptySummary);
        }

        if self.what_was_implemented.is_empty() {
            return Err(HandoffValidationError::EmptyImplementation);
        }

        if self.next_steps.is_empty() {
            return Err(HandoffValidationError::EmptyNextSteps);
        }

        Ok(())
    }

    /// 保存到文件。
    pub fn save_to(&self, handoffs_dir: &Path) -> MissionResult<PathBuf> {
        fs::create_dir_all(handoffs_dir).map_err(|source| MissionError::CreateHandoffDir {
            path: handoffs_dir.to_path_buf(),
            source,
        })?;

        let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let filename = format!(
            "{}-{}.{}",
            self.worker.replace('-', "_"),
            timestamp,
            HANDOFF_EXT
        );
        let path = handoffs_dir.join(&filename);

        let content = serde_json::to_string_pretty(self)
            .map_err(|source| MissionError::SerializeHandoff { source })?;

        fs::write(&path, content).map_err(|source| MissionError::WriteHandoff {
            path: path.clone(),
            source,
        })?;

        Ok(path)
    }

    /// 从文件加载。
    pub fn load_from(path: &Path) -> MissionResult<Self> {
        let content = fs::read_to_string(path).map_err(|source| MissionError::ReadHandoff {
            path: path.to_path_buf(),
            source,
        })?;

        serde_json::from_str(&content).map_err(|source| MissionError::ParseHandoff {
            path: path.to_path_buf(),
            source,
        })
    }

    /// 生成 Markdown 报告。
    pub fn to_markdown(&self) -> String {
        let mut report = String::new();

        report.push_str("# Worker Handoff Report\n\n");
        report.push_str(&format!("**Worker:** {}\n", self.worker));
        report.push_str(&format!("**Timestamp:** {}\n\n", self.timestamp));

        report.push_str("## Summary\n\n");
        report.push_str(&format!("{}\n\n", self.salient_summary));

        report.push_str("## What Was Implemented\n\n");
        for item in &self.what_was_implemented {
            report.push_str(&format!("- {}\n", item));
        }
        report.push('\n');

        if !self.files_modified.is_empty() {
            report.push_str("## Files Modified\n\n");
            for change in &self.files_modified {
                report.push_str(&format!(
                    "- **{}**: {}\n",
                    change.path, change.change_summary
                ));
            }
            report.push('\n');
        }

        if !self.files_created.is_empty() {
            report.push_str("## Files Created\n\n");
            for creation in &self.files_created {
                report.push_str(&format!("- **{}**: {}\n", creation.path, creation.purpose));
            }
            report.push('\n');
        }

        report.push_str("## Verification\n\n");

        report.push_str("### Code Review\n\n");
        report.push_str(&format!(
            "**Status:** {}\n",
            self.verification.code_review.status.label()
        ));
        report.push_str(&format!(
            "**Issues Found:** {}\n",
            self.verification.code_review.issues_found
        ));
        report.push_str(&format!(
            "**Issues Fixed:** {}\n",
            self.verification.code_review.issues_fixed
        ));
        report.push_str(&format!(
            "**Findings:** {}\n\n",
            self.verification.code_review.findings
        ));

        report.push_str("### User Testing\n\n");
        report.push_str(&format!(
            "**Status:** {}\n",
            self.verification.user_testing.status.label()
        ));
        report.push_str(&format!(
            "**Test Cases:** {}/{}\n",
            self.verification.user_testing.test_cases_passed,
            self.verification.user_testing.test_cases_executed
        ));
        report.push_str(&format!(
            "**Results:** {}\n\n",
            self.verification.user_testing.results
        ));

        if !self.verification.remaining_work.is_empty() {
            report.push_str(&format!(
                "### Remaining Work\n\n{}\n\n",
                self.verification.remaining_work
            ));
        }

        report.push_str("## Next Steps\n\n");
        report.push_str(&format!("{}\n\n", self.next_steps));

        if !self.blockers.is_empty() {
            report.push_str("## Blockers\n\n");
            for blocker in &self.blockers {
                report.push_str(&format!("- {}\n", blocker));
            }
            report.push('\n');
        }

        report
    }
}

impl Default for Verification {
    fn default() -> Self {
        Self {
            code_review: CodeReviewResult {
                status: ReviewStatus::Skipped,
                findings: String::new(),
                issues_found: 0,
                issues_fixed: 0,
            },
            user_testing: UserTestingResult {
                status: ReviewStatus::Skipped,
                results: String::new(),
                test_cases_executed: 0,
                test_cases_passed: 0,
            },
            remaining_work: String::new(),
        }
    }
}

/// Handoff 验证错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandoffValidationError {
    /// 核心摘要为空。
    EmptySummary,
    /// 实现内容为空。
    EmptyImplementation,
    /// 下一步建议为空。
    EmptyNextSteps,
}

impl std::fmt::Display for HandoffValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptySummary => write!(f, "核心摘要不能为空"),
            Self::EmptyImplementation => write!(f, "实现内容不能为空"),
            Self::EmptyNextSteps => write!(f, "下一步建议不能为空"),
        }
    }
}

impl std::error::Error for HandoffValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_handoff_with_builder() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Test completed successfully")
            .add_implementation("Feature A")
            .add_implementation("Feature B")
            .add_file_change("src/main.rs", "Add feature A")
            .add_file_creation("src/test.rs", "Test file for feature A")
            .with_next_steps("Continue with feature C")
            .add_blocker("Waiting for dependency");

        assert_eq!(handoff.worker, "test-worker");
        assert_eq!(handoff.salient_summary, "Test completed successfully");
        assert_eq!(handoff.what_was_implemented.len(), 2);
        assert_eq!(handoff.files_modified.len(), 1);
        assert_eq!(handoff.files_created.len(), 1);
        assert_eq!(handoff.next_steps, "Continue with feature C");
        assert_eq!(handoff.blockers.len(), 1);
    }

    #[test]
    fn validate_handoff_success() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Test completed")
            .add_implementation("Feature A")
            .with_next_steps("Next steps");

        assert!(handoff.validate().is_ok());
    }

    #[test]
    fn validate_handoff_failure() {
        let handoff = Handoff::new("test-worker".to_string());

        assert!(matches!(
            handoff.validate(),
            Err(HandoffValidationError::EmptySummary)
        ));
    }

    #[test]
    fn serialize_and_deserialize() {
        let original = Handoff::new("test-worker".to_string())
            .with_summary("Test completed")
            .add_implementation("Feature A")
            .with_next_steps("Next steps");

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: Handoff = serde_json::from_str(&json).unwrap();

        assert_eq!(original, deserialized);
    }

    #[test]
    fn to_markdown_contains_all_sections() {
        let handoff = Handoff::new("test-worker".to_string())
            .with_summary("Test completed")
            .add_implementation("Feature A")
            .add_file_change("src/main.rs", "Add feature")
            .add_file_creation("src/test.rs", "Test file")
            .with_next_steps("Next steps")
            .add_blocker("Blocker");

        let markdown = handoff.to_markdown();

        assert!(markdown.contains("# Worker Handoff Report"));
        assert!(markdown.contains("## Summary"));
        assert!(markdown.contains("## What Was Implemented"));
        assert!(markdown.contains("## Files Modified"));
        assert!(markdown.contains("## Files Created"));
        assert!(markdown.contains("## Verification"));
        assert!(markdown.contains("## Next Steps"));
        assert!(markdown.contains("## Blockers"));
    }
}

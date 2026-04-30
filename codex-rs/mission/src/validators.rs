//! Mission 验证器模块。
//!
//! 提供代码审查和用户测试验证功能。

mod scrutiny;
mod user_testing;

pub use scrutiny::ScrutinyReport;
pub use scrutiny::ScrutinyStatus;
pub use scrutiny::ScrutinyValidator;
pub use user_testing::UserTestingReport;
pub use user_testing::UserTestingValidator;

use crate::handoff::Handoff;

/// 验证器通用接口。
pub trait Validator {
    /// 验证报告类型。
    type Report;

    /// 验证 Handoff 并生成报告。
    fn validate(&self, handoff: &Handoff) -> Self::Report;
}

/// 验证器通用配置。
#[derive(Debug, Clone)]
pub struct ValidatorConfig {
    /// 是否严格模式（任何问题都导致失败）。
    pub strict: bool,
    /// 是否输出详细报告。
    pub verbose: bool,
}

impl Default for ValidatorConfig {
    fn default() -> Self {
        Self {
            strict: false,
            verbose: true,
        }
    }
}

/// 统一的问题严重程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    High,
    Medium,
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

/// 工具输出的规范类型
/// 为不同工具版本提供统一接口
use serde::Deserialize;
/// 工具输出的规范类型
/// 为不同工具版本提供统一接口
use serde::Serialize;

/// 测试执行结果（vitest、playwright、jest 等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub duration_ms: Option<u64>,
    pub failures: Vec<TestFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFailure {
    pub test_name: String,
    pub file_path: String,
    pub error_message: String,
    pub stack_trace: Option<String>,
}

/// Lint 结果（eslint、biome、tsc 等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintResult {
    pub total_files: usize,
    pub files_with_issues: usize,
    pub total_issues: usize,
    pub errors: usize,
    pub warnings: usize,
    pub issues: Vec<LintIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LintIssue {
    pub file_path: String,
    pub line: usize,
    pub column: usize,
    pub severity: LintSeverity,
    pub rule_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
    Info,
}

/// 依赖状态（pnpm、npm、cargo 等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyState {
    pub total_packages: usize,
    pub outdated_count: usize,
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub wanted_version: Option<String>,
    pub dev_dependency: bool,
}

/// 构建输出（next、webpack、vite、cargo 等）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildOutput {
    pub success: bool,
    pub duration_ms: Option<u64>,
    pub warnings: usize,
    pub errors: usize,
    pub bundles: Vec<BundleInfo>,
    pub routes: Vec<RouteInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundleInfo {
    pub name: String,
    pub size_bytes: u64,
    pub gzip_size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteInfo {
    pub path: String,
    pub size_kb: f64,
    pub first_load_js_kb: Option<f64>,
}

/// Git 操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitResult {
    pub operation: String,
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub commits: Vec<GitCommit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitCommit {
    pub hash: String,
    pub author: String,
    pub message: String,
    pub timestamp: Option<String>,
}

/// 通用命令输出（用于没有专用类型的工具）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenericOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub summary: Option<String>,
}

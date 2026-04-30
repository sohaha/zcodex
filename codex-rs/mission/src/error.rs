use std::path::PathBuf;

/// Mission 子系统的错误类型。
#[derive(Debug, thiserror::Error)]
pub enum MissionError {
    #[error("Mission 目标不能为空")]
    EmptyGoal,

    #[error("当前工作区还没有 Mission 状态文件：{path}")]
    MissingState { path: PathBuf },

    #[error("无法读取 Mission 状态文件 {path}: {source}")]
    ReadState {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法解析 Mission 状态文件 {path}: {source}")]
    ParseState {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("无法创建 Mission 状态目录 {path}: {source}")]
    CreateStateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法写入 Mission 状态文件 {path}: {source}")]
    WriteState {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法序列化 Mission 状态文件 {path}: {source}")]
    SerializeState {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("无法创建 Skill 目录 {path}: {source}")]
    CreateSkillDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法读取 Skill 目录 {path}: {source}")]
    ReadSkillDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法清理 Skill 目录 {path}: {source}")]
    CleanupSkillDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法读取 Skill 文件 {path}: {source}")]
    ReadSkillFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法写入 Skill 文件 {path}: {source}")]
    WriteSkillFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Skill 模板未找到: {name}")]
    SkillTemplateNotFound { name: String },

    #[error("Skill 模板无效: {name}, 原因: {reason}")]
    InvalidSkillTemplate { name: String, reason: String },

    #[error("Skill 文件未找到: {name}, 搜索路径: {path}")]
    SkillNotFound { name: String, path: PathBuf },

    #[error("Worker session 目录未找到: {path}")]
    WorkerSessionDirNotFound { path: PathBuf },

    #[error("无法创建 Worker session 目录 {path}: {source}")]
    CreateWorkerSessionDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法读取 Worker session {id}: {source}")]
    ReadWorkerSession {
        id: String,
        #[source]
        source: std::io::Error,
    },

    #[error("无法写入 Worker session {id}: {source}")]
    WriteWorkerSession {
        id: String,
        #[source]
        source: std::io::Error,
    },

    #[error("无法解析 Worker session {id}: {source}")]
    ParseWorkerSession {
        id: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("无法创建 Handoff 目录 {path}: {source}")]
    CreateHandoffDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法读取 Handoff 文件 {path}: {source}")]
    ReadHandoff {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法写入 Handoff 文件 {path}: {source}")]
    WriteHandoff {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法序列化 Handoff: {source}")]
    SerializeHandoff {
        #[source]
        source: serde_json::Error,
    },

    #[error("无法解析 Handoff 文件 {path}: {source}")]
    ParseHandoff {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("无法创建 .factory 目录 {path}: {source}")]
    CreateFactoryDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法读取 .factory 文件 {path}: {source}")]
    ReadFactoryFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法写入 .factory 文件 {path}: {source}")]
    WriteFactoryFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法读取 .factory 目录 {path}: {source}")]
    ReadFactoryDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法创建方案目录 {path}: {source}")]
    CreatePlanDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法写入方案文件 {path}: {source}")]
    WritePlan {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("无法读取方案文件 {path}: {source}")]
    ReadPlan {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("没有可执行的方案")]
    NoPlanToExecute,
}

/// Mission 子系统内部统一 Result。
pub type MissionResult<T> = Result<T, MissionError>;

/// 结构化输出解析时使用的错误类型
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("JSON 解析失败（行 {line}，列 {col}）：{msg}")]
    JsonError {
        line: usize,
        col: usize,
        msg: String,
    },

    #[error("模式不匹配：期望 {expected}")]
    PatternMismatch { expected: &'static str },

    #[error("解析不完整：已得到 {found}，缺失字段：{missing:?}")]
    PartialParse {
        found: String,
        missing: Vec<&'static str>,
    },

    #[error("无效格式：{0}")]
    InvalidFormat(String),

    #[error("缺少必填字段：{0}")]
    MissingField(&'static str),

    #[error("版本不匹配：得到 {got}，期望 {expected}")]
    VersionMismatch { got: String, expected: String },

    #[error("输出为空")]
    EmptyOutput,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<serde_json::Error> for ParseError {
    fn from(err: serde_json::Error) -> Self {
        ParseError::JsonError {
            line: err.line(),
            col: err.column(),
            msg: err.to_string(),
        }
    }
}

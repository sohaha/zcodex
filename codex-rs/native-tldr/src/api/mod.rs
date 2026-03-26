use serde::Deserialize;
use serde::Serialize;

use crate::lang_support::SupportedLanguage;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisKind {
    Ast,
    CallGraph,
    Cfg,
    Dfg,
    Pdg,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisRequest {
    pub kind: AnalysisKind,
    pub language: SupportedLanguage,
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisResponse {
    pub kind: AnalysisKind,
    pub summary: String,
}

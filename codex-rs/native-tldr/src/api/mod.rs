use serde::Deserialize;
use serde::Serialize;

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
    pub symbol: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisResponse {
    pub kind: AnalysisKind,
    pub summary: String,
}

impl AnalysisResponse {
    pub fn placeholder(kind: AnalysisKind) -> Self {
        Self {
            kind,
            summary: format!("{kind:?} analysis is not implemented yet"),
        }
    }
}

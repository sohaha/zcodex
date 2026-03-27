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
    Extract,
    Slice,
    ChangeImpact,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisRequest {
    pub kind: AnalysisKind,
    pub language: SupportedLanguage,
    pub symbol: Option<String>,
    pub path: Option<String>,
    pub line: Option<usize>,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisResponse {
    pub kind: AnalysisKind,
    pub summary: String,
    pub details: Option<AnalysisDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisDetail {
    pub indexed_files: usize,
    pub total_symbols: usize,
    pub symbol_query: Option<String>,
    pub truncated: bool,
    #[serde(default)]
    pub change_paths: Vec<String>,
    pub slice_target: Option<AnalysisSliceTarget>,
    #[serde(default)]
    pub slice_lines: Vec<usize>,
    #[serde(default)]
    pub overview: AnalysisOverviewDetail,
    #[serde(default)]
    pub files: Vec<AnalysisFileDetail>,
    #[serde(default)]
    pub nodes: Vec<AnalysisNodeDetail>,
    #[serde(default)]
    pub edges: Vec<AnalysisEdgeDetail>,
    #[serde(default)]
    pub symbol_index: Vec<AnalysisSymbolIndexEntry>,
    #[serde(default)]
    pub units: Vec<AnalysisUnitDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisSliceTarget {
    pub path: String,
    pub symbol: Option<String>,
    pub line: usize,
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisUnitDetail {
    pub path: String,
    pub line: usize,
    pub span_end_line: usize,
    pub symbol: Option<String>,
    pub qualified_symbol: Option<String>,
    pub kind: String,
    pub module_path: Vec<String>,
    pub visibility: Option<String>,
    pub signature: Option<String>,
    pub calls: Vec<String>,
    pub called_by: Vec<String>,
    pub references: Vec<String>,
    pub imports: Vec<String>,
    pub dependencies: Vec<String>,
    pub cfg_summary: String,
    pub dfg_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AnalysisOverviewDetail {
    #[serde(default)]
    pub kinds: Vec<AnalysisCountDetail>,
    pub outgoing_edges: usize,
    pub incoming_edges: usize,
    pub reference_count: usize,
    pub import_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisCountDetail {
    pub name: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisFileDetail {
    pub path: String,
    pub symbol_count: usize,
    pub kinds: Vec<AnalysisCountDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisEdgeDetail {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisNodeDetail {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub path: Option<String>,
    pub line: Option<usize>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnalysisSymbolIndexEntry {
    pub symbol: String,
    pub node_ids: Vec<String>,
}

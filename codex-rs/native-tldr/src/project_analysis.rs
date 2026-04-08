use crate::TldrConfig;
use crate::api::AnalysisCountDetail;
use crate::api::AnalysisDetail;
use crate::api::AnalysisEdgeDetail;
use crate::api::AnalysisKind;
use crate::api::AnalysisNodeDetail;
use crate::api::AnalysisOverviewDetail;
use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
use crate::api::AnalysisSymbolIndexEntry;
use crate::api::AnalysisUnitDetail;
use crate::semantic::EmbeddingUnit;
use crate::semantic::SemanticIndexer;
use anyhow::Result;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;

pub(crate) fn analyze_project_graph(
    project_root: &Path,
    config: &TldrConfig,
    request: AnalysisRequest,
) -> Result<AnalysisResponse> {
    let index = SemanticIndexer::new(config.semantic.clone())
        .load_or_build_index(project_root, request.language)?;
    let analysis = ProjectGraphAnalysis::new(index.indexed_files, index.units);
    let response = match request.kind {
        AnalysisKind::Calls => analysis.calls_response(),
        AnalysisKind::Impact => analysis.impact_response(request.symbol.as_deref()),
        AnalysisKind::Dead => analysis.dead_response(),
        AnalysisKind::Arch => analysis.arch_response(),
        _ => unreachable!("project graph analysis only supports graph kinds"),
    };
    Ok(response)
}

struct ProjectGraphAnalysis {
    indexed_files: usize,
    units: Vec<EmbeddingUnit>,
}

impl ProjectGraphAnalysis {
    fn new(indexed_files: usize, units: Vec<EmbeddingUnit>) -> Self {
        Self {
            indexed_files,
            units,
        }
    }

    fn calls_response(&self) -> AnalysisResponse {
        let edges = self.call_edges();
        let summary = format!(
            "calls summary: {} files, {} symbols, {} call edges",
            self.indexed_files,
            self.units.len(),
            edges.len()
        );
        AnalysisResponse {
            kind: AnalysisKind::Calls,
            summary,
            details: Some(self.graph_detail(
                None,
                edges,
                self.units.iter().collect(),
                "calls",
                Vec::new(),
            )),
        }
    }

    fn impact_response(&self, symbol: Option<&str>) -> AnalysisResponse {
        let target = symbol.unwrap_or("*");
        let callers = self
            .units
            .iter()
            .enumerate()
            .filter(|(_, unit)| contains_symbol(unit.calls.iter(), target))
            .collect::<Vec<_>>();
        let caller_units = callers.iter().map(|(_, unit)| *unit).collect::<Vec<_>>();
        let mut edges = Vec::new();
        let mut seen = BTreeSet::new();
        for (_, unit) in callers {
            let Some(from) = canonical_symbol(unit) else {
                continue;
            };
            for callee in &unit.calls {
                if matches_target(callee, target) && seen.insert((from.to_string(), callee.clone()))
                {
                    edges.push(AnalysisEdgeDetail {
                        from: from.to_string(),
                        to: callee.clone(),
                        kind: "calls".to_string(),
                    });
                }
            }
        }
        let summary = format!(
            "impact summary: {} callers found for {} across {} files",
            caller_units.len(),
            target,
            self.indexed_files
        );
        AnalysisResponse {
            kind: AnalysisKind::Impact,
            summary,
            details: Some(self.graph_detail(
                Some(target.to_string()),
                edges,
                caller_units,
                "impact",
                Vec::new(),
            )),
        }
    }

    fn dead_response(&self) -> AnalysisResponse {
        let dead_units = self
            .units
            .iter()
            .filter(|unit| unit.called_by.is_empty() && !is_entry_point(unit))
            .collect::<Vec<_>>();
        let dead_count = dead_units.len();
        let summary = format!(
            "dead summary: {} potentially unreachable symbols across {} files",
            dead_count, self.indexed_files
        );
        AnalysisResponse {
            kind: AnalysisKind::Dead,
            summary,
            details: Some(self.graph_detail(
                None,
                Vec::new(),
                dead_units,
                "dead",
                vec![AnalysisCountDetail {
                    name: "dead_symbols".to_string(),
                    count: dead_count,
                }],
            )),
        }
    }

    fn arch_response(&self) -> AnalysisResponse {
        let outgoing = self.outgoing_counts();
        let incoming = self.incoming_counts();
        let mut entry = 0usize;
        let mut middle = 0usize;
        let mut leaf = 0usize;
        let mut cycles = 0usize;
        let all_edges = self.call_edges();
        let edge_set = all_edges
            .iter()
            .map(|edge| (edge.from.clone(), edge.to.clone()))
            .collect::<BTreeSet<_>>();

        for unit in &self.units {
            let Some(symbol) = canonical_symbol(unit) else {
                continue;
            };
            let outgoing_count = *outgoing.get(symbol).unwrap_or(&0);
            let incoming_count = *incoming.get(symbol).unwrap_or(&0);
            if outgoing_count == 0 {
                leaf += 1;
            } else if incoming_count == 0 {
                entry += 1;
            } else {
                middle += 1;
            }
            if unit
                .calls
                .iter()
                .any(|callee| edge_set.contains(&(callee.clone(), symbol.to_string())))
            {
                cycles += 1;
            }
        }

        let summary = format!(
            "arch summary: {entry} entry, {middle} middle, {leaf} leaf symbols; {cycles} cyclic participants"
        );
        AnalysisResponse {
            kind: AnalysisKind::Arch,
            summary,
            details: Some(self.graph_detail(
                None,
                all_edges,
                self.units.iter().collect(),
                "arch",
                vec![
                    AnalysisCountDetail {
                        name: "entry".to_string(),
                        count: entry,
                    },
                    AnalysisCountDetail {
                        name: "middle".to_string(),
                        count: middle,
                    },
                    AnalysisCountDetail {
                        name: "leaf".to_string(),
                        count: leaf,
                    },
                    AnalysisCountDetail {
                        name: "cyclic".to_string(),
                        count: cycles,
                    },
                ],
            )),
        }
    }

    fn call_edges(&self) -> Vec<AnalysisEdgeDetail> {
        let mut seen = BTreeSet::new();
        let mut edges = Vec::new();
        for unit in &self.units {
            let Some(from) = canonical_symbol(unit) else {
                continue;
            };
            for callee in &unit.calls {
                let key = (from.to_string(), callee.clone());
                if seen.insert(key.clone()) {
                    edges.push(AnalysisEdgeDetail {
                        from: key.0,
                        to: key.1,
                        kind: "calls".to_string(),
                    });
                }
            }
        }
        edges
    }

    fn outgoing_counts(&self) -> BTreeMap<String, usize> {
        self.units
            .iter()
            .filter_map(|unit| {
                canonical_symbol(unit).map(|symbol| (symbol.to_string(), unit.calls.len()))
            })
            .collect()
    }

    fn incoming_counts(&self) -> BTreeMap<String, usize> {
        let mut incoming = BTreeMap::new();
        for unit in &self.units {
            for callee in &unit.calls {
                *incoming.entry(callee.clone()).or_default() += 1;
            }
        }
        incoming
    }

    fn graph_detail(
        &self,
        symbol_query: Option<String>,
        edges: Vec<AnalysisEdgeDetail>,
        units: Vec<&EmbeddingUnit>,
        overview_name: &str,
        kinds: Vec<AnalysisCountDetail>,
    ) -> AnalysisDetail {
        let mut node_ids = BTreeSet::new();
        let mut nodes = Vec::new();
        let mut symbol_index = BTreeMap::<String, Vec<String>>::new();
        for unit in &units {
            if let Some(symbol) = canonical_symbol(unit) {
                if node_ids.insert(symbol.to_string()) {
                    nodes.push(AnalysisNodeDetail {
                        id: symbol.to_string(),
                        label: unit.symbol.clone().unwrap_or_else(|| symbol.to_string()),
                        kind: unit.kind.clone(),
                        path: Some(unit.path.display().to_string()),
                        line: Some(unit.line),
                        signature: unit.signature.clone(),
                    });
                }
                symbol_index
                    .entry(symbol.to_string())
                    .or_default()
                    .push(symbol.to_string());
            }
        }
        AnalysisDetail {
            indexed_files: self.indexed_files,
            total_symbols: units.len(),
            symbol_query,
            truncated: false,
            change_paths: Vec::new(),
            slice_target: None,
            slice_lines: Vec::new(),
            overview: AnalysisOverviewDetail {
                kinds: if kinds.is_empty() {
                    vec![AnalysisCountDetail {
                        name: overview_name.to_string(),
                        count: units.len(),
                    }]
                } else {
                    kinds
                },
                outgoing_edges: edges.len(),
                incoming_edges: 0,
                reference_count: 0,
                import_count: units.iter().map(|unit| unit.imports.len()).sum(),
            },
            files: Vec::new(),
            nodes,
            edges,
            symbol_index: symbol_index
                .into_iter()
                .map(|(symbol, node_ids)| AnalysisSymbolIndexEntry { symbol, node_ids })
                .collect(),
            units: units.into_iter().map(analysis_unit_detail).collect(),
        }
    }
}

fn analysis_unit_detail(unit: &EmbeddingUnit) -> AnalysisUnitDetail {
    AnalysisUnitDetail {
        path: unit.path.display().to_string(),
        line: unit.line,
        span_end_line: unit.span_end_line,
        symbol: unit.symbol.clone(),
        qualified_symbol: unit.qualified_symbol.clone(),
        kind: unit.kind.clone(),
        owner_symbol: unit.owner_symbol.clone(),
        owner_kind: unit.owner_kind.clone(),
        implemented_trait: unit.implemented_trait.clone(),
        module_path: unit.module_path.clone(),
        visibility: unit.visibility.clone(),
        signature: unit.signature.clone(),
        calls: unit.calls.clone(),
        called_by: unit.called_by.clone(),
        references: unit.references.clone(),
        imports: unit.imports.clone(),
        dependencies: unit.dependencies.clone(),
        cfg_summary: unit.cfg_summary.clone(),
        dfg_summary: unit.dfg_summary.clone(),
    }
}

fn canonical_symbol(unit: &EmbeddingUnit) -> Option<&str> {
    unit.qualified_symbol
        .as_deref()
        .or(unit.symbol.as_deref())
        .or_else(|| unit.symbol_aliases.first().map(String::as_str))
}

fn matches_target(candidate: &str, target: &str) -> bool {
    candidate == target
        || candidate.rsplit("::").next() == Some(target)
        || candidate.rsplit('.').next() == Some(target)
}

fn contains_symbol<'a>(mut candidates: impl Iterator<Item = &'a String>, target: &str) -> bool {
    candidates.any(|candidate| matches_target(candidate, target))
}

fn is_entry_point(unit: &EmbeddingUnit) -> bool {
    matches!(
        unit.symbol.as_deref(),
        Some("main" | "run" | "init" | "setup" | "cli")
    ) || unit
        .symbol
        .as_deref()
        .is_some_and(|symbol| symbol.starts_with("test_"))
}

#[cfg(test)]
mod tests {
    use super::analyze_project_graph;
    use crate::TldrConfig;
    use crate::api::AnalysisKind;
    use crate::api::AnalysisRequest;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn impact_returns_reverse_call_graph() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn validate() {}\nfn login() {\n    validate();\n}\nfn audit() {\n    validate();\n}\n",
        )
        .expect("fixture should write");

        let response = analyze_project_graph(
            tempdir.path(),
            &TldrConfig::for_project(tempdir.path().to_path_buf()),
            AnalysisRequest {
                kind: AnalysisKind::Impact,
                language: SupportedLanguage::Rust,
                symbol: Some("validate".to_string()),
                path: None,
                line: None,
                paths: Vec::new(),
            },
        )
        .expect("impact should succeed");

        assert_eq!(response.kind, AnalysisKind::Impact);
        assert!(response.summary.contains("2 callers"));
    }
}

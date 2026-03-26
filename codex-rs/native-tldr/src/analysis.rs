use crate::TldrConfig;
use crate::api::AnalysisKind;
use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
use crate::semantic::EmbeddingUnit;
use crate::semantic::SemanticIndexer;
use anyhow::Result;
use std::collections::BTreeMap;
use std::path::Path;

pub(crate) fn analyze_project(
    project_root: &Path,
    config: &TldrConfig,
    request: AnalysisRequest,
) -> Result<AnalysisResponse> {
    let index = SemanticIndexer::new(config.semantic.clone())
        .build_index(project_root, request.language)?;
    let units = filter_symbol_units(&index.units, request.symbol.as_deref());
    let summary = match request.kind {
        AnalysisKind::Ast => {
            summarize_structure(index.indexed_files, &units, request.symbol.as_deref())
        }
        AnalysisKind::CallGraph => {
            summarize_context(index.indexed_files, &units, request.symbol.as_deref())
        }
        AnalysisKind::Cfg => summarize_cfg(index.indexed_files, &units, request.symbol.as_deref()),
        AnalysisKind::Dfg => summarize_dfg(index.indexed_files, &units, request.symbol.as_deref()),
        AnalysisKind::Pdg => summarize_pdg(index.indexed_files, &units, request.symbol.as_deref()),
    };

    Ok(AnalysisResponse {
        kind: request.kind,
        summary,
    })
}

fn filter_symbol_units<'a>(
    units: &'a [EmbeddingUnit],
    symbol: Option<&str>,
) -> Vec<&'a EmbeddingUnit> {
    match symbol {
        Some(symbol) => units
            .iter()
            .filter(|unit| symbol_matches(unit, symbol))
            .collect(),
        None => units.iter().filter(|unit| unit.symbol.is_some()).collect(),
    }
}

fn summarize_structure(
    indexed_files: usize,
    units: &[&EmbeddingUnit],
    symbol: Option<&str>,
) -> String {
    if let Some(symbol) = symbol {
        return summarize_symbol_lookup("structure", indexed_files, units, symbol, |unit| {
            format!(
                "{} {} @ {}:{}-{} module [{}] visibility [{}] signature [{}] calls [{}]",
                unit.kind,
                symbol_label(unit),
                unit.path.display(),
                unit.line,
                unit.span_end_line,
                join_or_none(&unit.module_path),
                unit.visibility.as_deref().unwrap_or("<none>"),
                unit.signature.as_deref().unwrap_or("<none>"),
                join_or_none(&unit.calls),
            )
        });
    }

    if units.is_empty() {
        return format!("structure summary: scanned {indexed_files} files but found no symbols");
    }

    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    for unit in units {
        *by_kind.entry(unit.kind.as_str()).or_default() += 1;
    }
    let kinds = by_kind
        .into_iter()
        .map(|(kind, count)| format!("{count} {kind}"))
        .collect::<Vec<_>>()
        .join(", ");
    let preview = units
        .iter()
        .take(5)
        .map(|unit| {
            format!(
                "{}@{}:{}-{}",
                symbol_label(unit),
                unit.path.display(),
                unit.line,
                unit.span_end_line,
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "structure summary: {} symbols across {} files ({kinds}); sample: {preview}",
        units.len(),
        indexed_files
    )
}

fn summarize_context(
    indexed_files: usize,
    units: &[&EmbeddingUnit],
    symbol: Option<&str>,
) -> String {
    if let Some(symbol) = symbol {
        return summarize_symbol_lookup("context", indexed_files, units, symbol, |unit| {
            format!(
                "{} @ {}:{} outgoing [{}]; incoming [{}]; refs [{}]",
                symbol_label(unit),
                unit.path.display(),
                unit.line,
                join_or_none(&unit.calls),
                join_or_none(&unit.called_by),
                join_or_none(&unit.references),
            )
        });
    }

    if units.is_empty() {
        return format!("context summary: scanned {indexed_files} files but found no symbols");
    }

    let outgoing = units.iter().map(|unit| unit.calls.len()).sum::<usize>();
    let incoming = units.iter().map(|unit| unit.called_by.len()).sum::<usize>();
    let hotspots = units
        .iter()
        .filter(|unit| !unit.calls.is_empty() || !unit.called_by.is_empty())
        .take(5)
        .map(|unit| {
            format!(
                "{}(out={},in={})",
                symbol_label(unit),
                unit.calls.len(),
                unit.called_by.len()
            )
        })
        .collect::<Vec<_>>();
    let hotspot_text = if hotspots.is_empty() {
        "none".to_string()
    } else {
        hotspots.join(", ")
    };
    format!(
        "context summary: {} symbols across {} files; outgoing edges={outgoing}, incoming edges={incoming}; hotspots: {hotspot_text}",
        units.len(),
        indexed_files
    )
}

fn summarize_cfg(indexed_files: usize, units: &[&EmbeddingUnit], symbol: Option<&str>) -> String {
    if let Some(symbol) = symbol {
        return summarize_symbol_lookup("cfg", indexed_files, units, symbol, |unit| {
            format!(
                "{} @ {}:{} => {}",
                symbol_label(unit),
                unit.path.display(),
                unit.line,
                unit.cfg_summary
            )
        });
    }

    if units.is_empty() {
        return format!("cfg summary: scanned {indexed_files} files but found no symbols");
    }

    let preview = units
        .iter()
        .take(5)
        .map(|unit| {
            format!(
                "{}:{} [{}]",
                unit.path.display(),
                unit.line,
                unit.cfg_summary
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "cfg summary: {} symbols across {} files; sample: {preview}",
        units.len(),
        indexed_files
    )
}

fn summarize_dfg(indexed_files: usize, units: &[&EmbeddingUnit], symbol: Option<&str>) -> String {
    if let Some(symbol) = symbol {
        return summarize_symbol_lookup("dfg", indexed_files, units, symbol, |unit| {
            format!(
                "{} @ {}:{} => {}; refs [{}]",
                symbol_label(unit),
                unit.path.display(),
                unit.line,
                unit.dfg_summary,
                join_or_none(&unit.references),
            )
        });
    }

    if units.is_empty() {
        return format!("dfg summary: scanned {indexed_files} files but found no symbols");
    }

    let assignment_like = units
        .iter()
        .filter(|unit| unit.dfg_summary.contains("assignments"))
        .count();
    format!(
        "dfg summary: {} symbols across {} files; {} previews contain local assignments",
        units.len(),
        indexed_files,
        assignment_like
    )
}

fn summarize_pdg(indexed_files: usize, units: &[&EmbeddingUnit], symbol: Option<&str>) -> String {
    if let Some(symbol) = symbol {
        return summarize_symbol_lookup("impact", indexed_files, units, symbol, |unit| {
            format!(
                "{} @ {}:{} deps [{}]; imports [{}]; outgoing [{}]; incoming [{}]; refs [{}]; {}",
                symbol_label(unit),
                unit.path.display(),
                unit.line,
                join_or_none(&unit.dependencies),
                join_or_none(&unit.imports),
                join_or_none(&unit.calls),
                join_or_none(&unit.called_by),
                join_or_none(&unit.references),
                unit.dfg_summary,
            )
        });
    }

    if units.is_empty() {
        return format!("impact summary: scanned {indexed_files} files but found no symbols");
    }

    let touched_paths = units
        .iter()
        .map(|unit| unit.path.display().to_string())
        .collect::<Vec<_>>();
    let unique_paths = touched_paths
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let edges = units
        .iter()
        .map(|unit| unit.calls.len() + unit.called_by.len())
        .sum::<usize>();
    format!(
        "impact summary: {} symbols across {} files ({} touched paths); dependency edges={edges}",
        units.len(),
        indexed_files,
        unique_paths
    )
}

fn summarize_symbol_lookup(
    label: &str,
    indexed_files: usize,
    units: &[&EmbeddingUnit],
    symbol: &str,
    describe: impl Fn(&EmbeddingUnit) -> String,
) -> String {
    if units.is_empty() {
        return format!(
            "{label} summary: symbol `{symbol}` not found in {indexed_files} indexed files"
        );
    }

    let matches = units
        .iter()
        .map(|unit| describe(unit))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "{label} summary: found {} match(es) for `{symbol}` in {indexed_files} indexed files; {matches}",
        units.len()
    )
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn symbol_matches(unit: &EmbeddingUnit, symbol: &str) -> bool {
    unit.symbol.as_deref() == Some(symbol)
        || unit.qualified_symbol.as_deref() == Some(symbol)
        || unit.symbol_aliases.iter().any(|alias| alias == symbol)
}

fn symbol_label(unit: &EmbeddingUnit) -> &str {
    unit.qualified_symbol
        .as_deref()
        .or(unit.symbol.as_deref())
        .unwrap_or("<file>")
}

#[cfg(test)]
mod tests {
    use super::analyze_project;
    use crate::TldrConfig;
    use crate::api::AnalysisKind;
    use crate::api::AnalysisRequest;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn structure_analysis_summarizes_symbols() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn login() {\n    validate(user);\n}\n\nfn validate(user: &str) {\n    println!(\"{}\", user);\n}\n",
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::Ast,
                language: SupportedLanguage::Rust,
                symbol: None,
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Ast);
        assert!(response.summary.contains("structure summary:"));
        assert!(response.summary.contains("2 symbols across 1 files"));
        assert!(response.summary.contains("login@src/lib.rs:1"));
    }

    #[test]
    fn context_analysis_tracks_incoming_and_outgoing_edges() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn login() {\n    validate(user);\n}\n\nfn validate(user: &str) {\n    println!(\"{}\", user);\n}\n",
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::CallGraph,
                language: SupportedLanguage::Rust,
                symbol: Some("validate".to_string()),
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::CallGraph);
        assert!(response.summary.contains("context summary:"));
        assert!(response.summary.contains("incoming [login]"));
    }

    #[test]
    fn impact_analysis_reports_missing_symbols_cleanly() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(tempdir.path().join("src/lib.rs"), "fn login() {}\n")
            .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::Pdg,
                language: SupportedLanguage::Rust,
                symbol: Some("logout".to_string()),
            },
        )
        .expect("analysis should succeed");

        assert_eq!(
            response.summary,
            "impact summary: symbol `logout` not found in 1 indexed files"
        );
    }

    #[test]
    fn structure_analysis_supports_qualified_symbol_lookup() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            r#"
mod auth {
    struct AuthService;

    impl AuthService {
        fn login(&self) {
            self.validate();
        }

        fn validate(&self) {}
    }
}
"#,
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::Ast,
                language: SupportedLanguage::Rust,
                symbol: Some("auth::AuthService::login".to_string()),
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Ast);
        assert!(response.summary.contains("auth::AuthService::login"));
        assert!(response.summary.contains("signature [fn login(&self)]"));
    }
}

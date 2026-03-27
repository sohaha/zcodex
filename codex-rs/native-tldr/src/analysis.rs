use crate::TldrConfig;
use crate::api::AnalysisCountDetail;
use crate::api::AnalysisDetail;
use crate::api::AnalysisEdgeDetail;
use crate::api::AnalysisFileDetail;
use crate::api::AnalysisKind;
use crate::api::AnalysisNodeDetail;
use crate::api::AnalysisOverviewDetail;
use crate::api::AnalysisRequest;
use crate::api::AnalysisResponse;
use crate::api::AnalysisSliceTarget;
use crate::api::AnalysisSymbolIndexEntry;
use crate::api::AnalysisUnitDetail;
use crate::semantic::EmbeddingUnit;
use crate::semantic::SemanticIndexer;
use anyhow::Result;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

pub(crate) fn analyze_project(
    project_root: &Path,
    config: &TldrConfig,
    request: AnalysisRequest,
) -> Result<AnalysisResponse> {
    let index = SemanticIndexer::new(config.semantic.clone())
        .build_index(project_root, request.language)?;
    let path = request
        .path
        .as_deref()
        .map(|value| normalize_request_path(project_root, value))
        .transpose()?;
    let change_paths = request
        .paths
        .iter()
        .map(|value| normalize_request_path(project_root, value))
        .collect::<Result<Vec<_>>>()?;
    let units = filter_units(
        &index.units,
        request.symbol.as_deref(),
        path.as_deref(),
        matches!(request.kind, AnalysisKind::Extract | AnalysisKind::Slice),
    );
    let units = if request.kind == AnalysisKind::ChangeImpact {
        filter_change_impact_units(&index.units, &change_paths)
    } else {
        units
    };
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
        AnalysisKind::Extract => summarize_extract(index.indexed_files, &units, path.as_deref()),
        AnalysisKind::Slice => summarize_slice(
            index.indexed_files,
            &index.units,
            &units,
            path.as_deref(),
            request.symbol.as_deref(),
            request.line,
        )?,
        AnalysisKind::ChangeImpact => {
            summarize_change_impact(index.indexed_files, &index.units, &change_paths)
        }
    };

    Ok(AnalysisResponse {
        kind: request.kind,
        summary,
        details: Some(build_analysis_detail(
            index.indexed_files,
            &index.units,
            &units,
            request.symbol.clone(),
            &change_paths,
            path.as_deref(),
            request.line,
            request.kind == AnalysisKind::Slice,
        )),
    })
}

fn build_analysis_detail(
    indexed_files: usize,
    all_units: &[EmbeddingUnit],
    units: &[&EmbeddingUnit],
    symbol_query: Option<String>,
    change_paths: &[PathBuf],
    path: Option<&Path>,
    line: Option<usize>,
    include_slice: bool,
) -> AnalysisDetail {
    const MAX_UNITS: usize = 20;
    const MAX_FILES: usize = 20;
    const MAX_EDGES: usize = 50;

    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_file: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut node_by_id: BTreeMap<String, AnalysisNodeDetail> = BTreeMap::new();
    let mut symbol_index: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut edge_keys = BTreeSet::new();
    let mut edges = Vec::new();
    let mut outgoing_edges = 0;
    let mut incoming_edges = 0;
    let mut reference_count = 0;
    let mut import_count = 0;
    let slice_target = include_slice.then(|| AnalysisSliceTarget {
        path: path
            .map(|value| value.display().to_string())
            .unwrap_or_else(|| "<unknown>".to_string()),
        symbol: symbol_query.clone(),
        line: line.unwrap_or_default(),
        direction: "backward".to_string(),
    });
    let slice_lines = if include_slice {
        compute_slice_lines(all_units, units, path, line)
    } else {
        Vec::new()
    };

    for unit in units {
        *by_kind.entry(unit.kind.clone()).or_default() += 1;
        let file_path = unit.path.display().to_string();
        let file_entry = by_file.entry(file_path.clone()).or_default();
        *file_entry.entry(unit.kind.clone()).or_default() += 1;

        let from = graph_node_name(unit);
        ensure_external_node_with_kind(&mut node_by_id, &file_path, "file");
        if let Some(from_id) = &from {
            upsert_unit_node(&mut node_by_id, unit, from_id);
            push_edge(
                &mut edges,
                &mut edge_keys,
                file_path.clone(),
                from_id.clone(),
                "contains",
            );
            if let Some(symbol) = unit.symbol.as_ref().or(unit.qualified_symbol.as_ref()) {
                symbol_index
                    .entry(symbol.clone())
                    .or_default()
                    .insert(from_id.clone());
            }
        }
        for call in &unit.calls {
            outgoing_edges += 1;
            if let Some(from) = &from {
                ensure_external_node(&mut node_by_id, call);
                push_edge(
                    &mut edges,
                    &mut edge_keys,
                    from.clone(),
                    call.clone(),
                    "calls",
                );
            }
        }
        for caller in &unit.called_by {
            incoming_edges += 1;
            if let Some(to) = &from {
                ensure_external_node(&mut node_by_id, caller);
                push_edge(
                    &mut edges,
                    &mut edge_keys,
                    caller.clone(),
                    to.clone(),
                    "calls",
                );
            }
        }
        for import in &unit.imports {
            if let Some(from) = &from {
                ensure_external_node_with_kind(&mut node_by_id, import, "import");
                push_edge(
                    &mut edges,
                    &mut edge_keys,
                    from.clone(),
                    import.clone(),
                    "imports",
                );
            }
        }
        for reference in &unit.references {
            if let Some(from) = &from {
                ensure_external_node_with_kind(&mut node_by_id, reference, "reference");
                push_edge(
                    &mut edges,
                    &mut edge_keys,
                    from.clone(),
                    reference.clone(),
                    "references",
                );
            }
        }
        reference_count += unit.references.len();
        import_count += unit.imports.len();
    }

    AnalysisDetail {
        indexed_files,
        total_symbols: units.len(),
        symbol_query,
        truncated: units.len() > MAX_UNITS,
        change_paths: change_paths
            .iter()
            .map(|value| value.display().to_string())
            .collect(),
        slice_target,
        slice_lines,
        overview: AnalysisOverviewDetail {
            kinds: by_kind
                .into_iter()
                .map(|(name, count)| AnalysisCountDetail { name, count })
                .collect(),
            outgoing_edges,
            incoming_edges,
            reference_count,
            import_count,
        },
        files: by_file
            .into_iter()
            .take(MAX_FILES)
            .map(|(path, kinds)| AnalysisFileDetail {
                symbol_count: kinds.values().sum(),
                path,
                kinds: kinds
                    .into_iter()
                    .map(|(name, count)| AnalysisCountDetail { name, count })
                    .collect(),
            })
            .collect(),
        nodes: node_by_id.into_values().collect(),
        edges: edges.into_iter().take(MAX_EDGES).collect(),
        symbol_index: symbol_index
            .into_iter()
            .map(|(symbol, node_ids)| AnalysisSymbolIndexEntry {
                symbol,
                node_ids: node_ids.into_iter().collect(),
            })
            .collect(),
        units: units
            .iter()
            .take(MAX_UNITS)
            .map(|unit| AnalysisUnitDetail {
                path: unit.path.display().to_string(),
                line: unit.line,
                span_end_line: unit.span_end_line,
                symbol: unit.symbol.clone(),
                qualified_symbol: unit.qualified_symbol.clone(),
                kind: unit.kind.clone(),
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
            })
            .collect(),
    }
}

fn graph_node_name(unit: &EmbeddingUnit) -> Option<String> {
    unit.qualified_symbol
        .clone()
        .or_else(|| unit.symbol.clone())
        .or_else(|| {
            if unit.kind == "module" {
                Some(unit.path.display().to_string())
            } else {
                None
            }
        })
}

fn upsert_unit_node(
    node_by_id: &mut BTreeMap<String, AnalysisNodeDetail>,
    unit: &EmbeddingUnit,
    id: &str,
) {
    let detail = unit_node_detail(unit, id);
    match node_by_id.get_mut(id) {
        Some(existing) if should_upgrade_node(existing, &detail) => *existing = detail,
        Some(_) => {}
        None => {
            node_by_id.insert(id.to_string(), detail);
        }
    }
}

fn should_upgrade_node(existing: &AnalysisNodeDetail, candidate: &AnalysisNodeDetail) -> bool {
    existing.path.is_none() && candidate.path.is_some()
}

fn push_edge(
    edges: &mut Vec<AnalysisEdgeDetail>,
    edge_keys: &mut BTreeSet<(String, String, String)>,
    from: String,
    to: String,
    kind: &str,
) {
    let key = (from.clone(), to.clone(), kind.to_string());
    if edge_keys.insert(key) {
        edges.push(AnalysisEdgeDetail {
            from,
            to,
            kind: kind.to_string(),
        });
    }
}

fn unit_node_detail(unit: &EmbeddingUnit, id: &str) -> AnalysisNodeDetail {
    AnalysisNodeDetail {
        id: id.to_string(),
        label: unit
            .qualified_symbol
            .as_ref()
            .or(unit.symbol.as_ref())
            .cloned()
            .unwrap_or_else(|| id.to_string()),
        kind: unit.kind.clone(),
        path: Some(unit.path.display().to_string()),
        line: Some(unit.line),
        signature: unit.signature.clone(),
    }
}

fn ensure_external_node(node_by_id: &mut BTreeMap<String, AnalysisNodeDetail>, id: &str) {
    ensure_external_node_with_kind(node_by_id, id, "symbol");
}

fn ensure_external_node_with_kind(
    node_by_id: &mut BTreeMap<String, AnalysisNodeDetail>,
    id: &str,
    kind: &str,
) {
    node_by_id
        .entry(id.to_string())
        .or_insert_with(|| AnalysisNodeDetail {
            id: id.to_string(),
            label: id.to_string(),
            kind: kind.to_string(),
            path: None,
            line: None,
            signature: None,
        });
}

fn filter_units<'a>(
    units: &'a [EmbeddingUnit],
    symbol: Option<&str>,
    path: Option<&Path>,
    include_symbol_less: bool,
) -> Vec<&'a EmbeddingUnit> {
    units
        .iter()
        .filter(|unit| path.is_none_or(|expected| unit.path == expected))
        .filter(|unit| match symbol {
            Some(symbol) => symbol_matches(unit, symbol),
            None => include_symbol_less || unit.symbol.is_some(),
        })
        .collect()
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

fn summarize_extract(
    indexed_files: usize,
    units: &[&EmbeddingUnit],
    path: Option<&Path>,
) -> String {
    let path_label = path
        .map(|value| value.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    if units.is_empty() {
        return format!(
            "extract summary: {path_label} was not found in {indexed_files} indexed files"
        );
    }

    let symbol_units = units
        .iter()
        .copied()
        .filter(|unit| unit.symbol.is_some())
        .collect::<Vec<_>>();
    if symbol_units.is_empty() {
        let preview = units
            .iter()
            .map(|unit| {
                format!(
                    "{}:{}-{}",
                    unit.path.display(),
                    unit.line,
                    unit.span_end_line
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        return format!("extract summary: {path_label} has no indexed symbols; preview: {preview}");
    }

    let mut by_kind: BTreeMap<&str, usize> = BTreeMap::new();
    let import_count = symbol_units
        .iter()
        .map(|unit| unit.imports.len())
        .sum::<usize>();
    let reference_count = symbol_units
        .iter()
        .map(|unit| unit.references.len())
        .sum::<usize>();
    for unit in &symbol_units {
        *by_kind.entry(unit.kind.as_str()).or_default() += 1;
    }
    let kinds = by_kind
        .into_iter()
        .map(|(kind, count)| format!("{count} {kind}"))
        .collect::<Vec<_>>()
        .join(", ");
    let preview = symbol_units
        .iter()
        .take(5)
        .map(|unit| {
            format!(
                "{}:{}-{}",
                symbol_label(unit),
                unit.line,
                unit.span_end_line
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "extract summary: {path_label} => {} symbols ({kinds}); imports={import_count}, references={reference_count}; sample: {preview}",
        symbol_units.len()
    )
}

fn summarize_change_impact(
    indexed_files: usize,
    all_units: &[EmbeddingUnit],
    change_paths: &[PathBuf],
) -> String {
    if change_paths.is_empty() {
        return "change-impact summary: no changed paths were provided".to_string();
    }

    let changed = all_units
        .iter()
        .filter(|unit| change_paths.contains(&unit.path))
        .collect::<Vec<_>>();
    let mut impacted = BTreeSet::new();
    for unit in &changed {
        if let Some(symbol) = unit.symbol.as_ref().or(unit.qualified_symbol.as_ref()) {
            impacted.insert(symbol.clone());
        }
        for caller in &unit.called_by {
            impacted.insert(caller.clone());
        }
    }
    format!(
        "change-impact summary: {} changed paths -> {} impacted symbols across {} indexed files",
        change_paths.len(),
        impacted.len(),
        indexed_files
    )
}

fn summarize_slice(
    indexed_files: usize,
    all_units: &[EmbeddingUnit],
    units: &[&EmbeddingUnit],
    path: Option<&Path>,
    symbol: Option<&str>,
    line: Option<usize>,
) -> Result<String> {
    let line = line.ok_or_else(|| anyhow::anyhow!("`line` is required for action=slice"))?;
    let path_label = path
        .map(|value| value.display().to_string())
        .unwrap_or_else(|| "<unknown>".to_string());
    if units.is_empty() {
        let symbol_label = symbol.unwrap_or("<unknown>");
        return Ok(format!(
            "slice summary: symbol `{symbol_label}` not found in {path_label} ({indexed_files} indexed files)"
        ));
    }

    let slice_lines = compute_slice_lines(all_units, units, path, Some(line));
    let preview = slice_lines
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "slice summary: backward slice for {}:{}:{} -> {} lines [{}]",
        path_label,
        symbol.unwrap_or("<unknown>"),
        line,
        slice_lines.len(),
        preview
    ))
}

fn compute_slice_lines(
    all_units: &[EmbeddingUnit],
    units: &[&EmbeddingUnit],
    path: Option<&Path>,
    line: Option<usize>,
) -> Vec<usize> {
    let Some(target_line) = line else {
        return Vec::new();
    };
    let Some(target_unit) = units
        .iter()
        .copied()
        .find(|unit| unit.line <= target_line && unit.span_end_line >= target_line)
        .or_else(|| units.first().copied())
    else {
        return Vec::new();
    };

    let mut slice_lines = BTreeSet::from([target_line, target_unit.line]);
    let related_symbols = target_unit
        .calls
        .iter()
        .chain(target_unit.called_by.iter())
        .cloned()
        .collect::<BTreeSet<_>>();
    for unit in all_units
        .iter()
        .filter(|unit| path.is_none_or(|expected| unit.path == expected))
    {
        if unit.line > target_line {
            continue;
        }
        if unit.path == target_unit.path
            && (unit.line == target_unit.line
                || unit
                    .symbol
                    .as_ref()
                    .is_some_and(|symbol| related_symbols.contains(symbol))
                || unit
                    .qualified_symbol
                    .as_ref()
                    .is_some_and(|symbol| related_symbols.contains(symbol)))
        {
            slice_lines.insert(unit.line);
        }
    }
    slice_lines.into_iter().collect()
}

fn filter_change_impact_units<'a>(
    units: &'a [EmbeddingUnit],
    change_paths: &[PathBuf],
) -> Vec<&'a EmbeddingUnit> {
    let changed = units
        .iter()
        .filter(|unit| change_paths.contains(&unit.path))
        .collect::<Vec<_>>();
    let impacted_symbols = changed
        .iter()
        .flat_map(|unit| {
            unit.symbol
                .iter()
                .chain(unit.qualified_symbol.iter())
                .chain(unit.called_by.iter())
        })
        .cloned()
        .collect::<BTreeSet<_>>();
    units
        .iter()
        .filter(|unit| {
            change_paths.contains(&unit.path)
                || unit
                    .symbol
                    .as_ref()
                    .is_some_and(|symbol| impacted_symbols.contains(symbol))
                || unit
                    .qualified_symbol
                    .as_ref()
                    .is_some_and(|symbol| impacted_symbols.contains(symbol))
        })
        .collect()
}

fn normalize_request_path(project_root: &Path, path: &str) -> Result<PathBuf> {
    let path = PathBuf::from(path);
    let absolute = if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    };
    let normalized = absolute.canonicalize().unwrap_or(absolute);
    Ok(normalized
        .strip_prefix(project_root)
        .map(Path::to_path_buf)
        .unwrap_or(normalized))
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
    use crate::api::AnalysisSliceTarget;
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
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Ast);
        assert!(response.summary.contains("structure summary:"));
        assert!(response.summary.contains("2 symbols across 1 files"));
        assert!(response.summary.contains("login@src/lib.rs:1"));
        let details = response.details.expect("details should exist");

        assert_eq!(details.indexed_files, 1);
        assert_eq!(details.total_symbols, 2);
        assert_eq!(details.overview.kinds[0].name, "function");
        assert_eq!(details.files[0].path, "src/lib.rs");
        assert_eq!(details.units[0].symbol.as_deref(), Some("login"));
        assert_eq!(details.nodes[0].kind, "function");
        assert_eq!(details.symbol_index[0].symbol, "login");
        assert!(
            details
                .edges
                .iter()
                .any(|edge| edge.kind == "contains" && edge.from == "src/lib.rs")
        );
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
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::CallGraph);
        assert!(response.summary.contains("context summary:"));
        assert!(response.summary.contains("incoming [login]"));
        let details = response.details.expect("details should exist");
        assert_eq!(details.symbol_query.as_deref(), Some("validate"));
        assert_eq!(details.units.len(), 1);
        assert_eq!(details.units[0].called_by, vec!["login".to_string()]);
        assert_eq!(details.overview.incoming_edges, 1);
        assert!(
            details
                .edges
                .iter()
                .any(|edge| edge.from == "login" && edge.to == "validate")
        );
        assert!(
            details
                .edges
                .iter()
                .any(|edge| edge.kind == "contains" && edge.to == "validate")
        );
        assert!(details.nodes.iter().any(|node| node.id == "login"));
        assert!(details.nodes.iter().any(|node| node.id == "validate"));
    }

    #[test]
    fn context_analysis_deduplicates_call_edges_across_calls_and_called_by() {
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
                symbol: None,
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        let details = response.details.expect("details should exist");
        let call_edges = details
            .edges
            .iter()
            .filter(|edge| edge.kind == "calls" && edge.from == "login" && edge.to == "validate")
            .count();
        assert_eq!(call_edges, 1);
    }

    #[test]
    fn structure_analysis_promotes_placeholder_nodes_to_real_symbol_kinds() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn login() {\n    validate();\n}\n\nfn validate() {}\n",
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
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        let details = response.details.expect("details should exist");
        let validate = details
            .nodes
            .iter()
            .find(|node| node.id == "validate")
            .expect("validate node should exist");
        assert_eq!(validate.kind, "function");
        assert_eq!(validate.path.as_deref(), Some("src/lib.rs"));
        assert_eq!(validate.line, Some(5));
    }

    #[test]
    fn extract_analysis_filters_to_requested_file() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(tempdir.path().join("src/lib.rs"), "fn login() {}\n")
            .expect("lib fixture should write");
        std::fs::write(tempdir.path().join("src/other.rs"), "fn logout() {}\n")
            .expect("other fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::Extract,
                language: SupportedLanguage::Rust,
                symbol: None,
                path: Some("src/lib.rs".to_string()),
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Extract);
        assert_eq!(
            response.summary,
            "extract summary: src/lib.rs => 1 symbols (1 function); imports=0, references=0; sample: login:1-1"
        );
        let details = response.details.expect("details should exist");
        assert_eq!(details.indexed_files, 2);
        assert_eq!(details.total_symbols, 1);
        assert_eq!(details.files.len(), 1);
        assert_eq!(details.files[0].path, "src/lib.rs");
        assert_eq!(details.units.len(), 1);
        assert_eq!(details.units[0].symbol.as_deref(), Some("login"));
    }

    #[test]
    fn slice_analysis_reports_backward_lines_for_requested_symbol() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn validate() {}\n\nfn login() {\n    validate();\n}\n",
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::Slice,
                language: SupportedLanguage::Rust,
                symbol: Some("login".to_string()),
                path: Some("src/lib.rs".to_string()),
                paths: Vec::new(),
                line: Some(4),
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Slice);
        assert_eq!(
            response.summary,
            "slice summary: backward slice for src/lib.rs:login:4 -> 3 lines [1, 3, 4]"
        );
        let details = response.details.expect("details should exist");
        assert_eq!(
            details.slice_target,
            Some(AnalysisSliceTarget {
                path: "src/lib.rs".to_string(),
                symbol: Some("login".to_string()),
                line: 4,
                direction: "backward".to_string(),
            })
        );
        assert_eq!(details.slice_lines, vec![1, 3, 4]);
    }

    #[test]
    fn change_impact_analysis_summarizes_impacted_callers() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn validate() {}\n\nfn login() {\n    validate();\n}\n",
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::ChangeImpact,
                language: SupportedLanguage::Rust,
                symbol: None,
                path: None,
                line: None,
                paths: vec!["src/lib.rs".to_string()],
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::ChangeImpact);
        assert_eq!(
            response.summary,
            "change-impact summary: 1 changed paths -> 2 impacted symbols across 1 indexed files"
        );
        let details = response.details.expect("details should exist");
        assert_eq!(details.change_paths, vec!["src/lib.rs".to_string()]);
        assert!(
            details
                .units
                .iter()
                .any(|unit| unit.symbol.as_deref() == Some("validate"))
        );
        assert!(
            details
                .units
                .iter()
                .any(|unit| unit.symbol.as_deref() == Some("login"))
        );
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
                path: None,
                paths: Vec::new(),

                line: None,
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
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        assert_eq!(response.kind, AnalysisKind::Ast);
        assert!(response.summary.contains("auth::AuthService::login"));
        assert!(response.summary.contains("signature [fn login(&self)]"));
        let details = response.details.expect("details should exist");
        assert_eq!(
            details.units[0].qualified_symbol.as_deref(),
            Some("auth::AuthService::login")
        );
        assert_eq!(details.files[0].symbol_count, 1);
        assert_eq!(details.nodes[0].id, "auth::AuthService::login");
    }

    #[test]
    fn structure_analysis_emits_import_and_reference_edges() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "use crate::auth::Session;\nfn login(session: Session) { println!(\"{:?}\", session); }\n",
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let response = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::Ast,
                language: SupportedLanguage::Rust,
                symbol: Some("login".to_string()),
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        let details = response.details.expect("details should exist");
        assert!(
            details
                .edges
                .iter()
                .any(|edge| edge.kind == "imports" && edge.to.contains("use crate::auth::Session"))
        );
        assert!(
            details
                .edges
                .iter()
                .any(|edge| edge.kind == "references" && edge.to == "Session")
        );
        assert!(
            details
                .nodes
                .iter()
                .any(|node| node.kind == "import" && node.id.contains("use crate::auth::Session"))
        );
        assert!(
            details
                .nodes
                .iter()
                .any(|node| node.kind == "reference" && node.id == "Session")
        );
    }

    #[test]
    fn structure_analysis_handles_qualified_symbol_import_reference_and_calls_together() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            r#"
mod auth {
    #[derive(Debug)]
    pub struct Session;

    pub struct AuthService;

    impl AuthService {
        pub fn login(&self, session: Session) {
            self.validate(&session);
        }

        fn validate(&self, session: &Session) {
            let _ = session;
        }
    }
}

use crate::auth::{AuthService, Session};

fn login_flow(service: &AuthService, session: Session) {
    service.login(session);
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
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");

        let details = response.details.expect("details should exist");
        assert_eq!(
            details.symbol_query.as_deref(),
            Some("auth::AuthService::login")
        );
        assert_eq!(
            details.units[0].qualified_symbol.as_deref(),
            Some("auth::AuthService::login")
        );
        assert!(details.edges.iter().any(|edge| {
            edge.kind == "imports" && edge.to.contains("use crate::auth::{AuthService, Session};")
        }));
        assert!(
            details
                .edges
                .iter()
                .any(|edge| edge.kind == "references" && edge.to == "Session")
        );
        assert!(details.edges.iter().any(|edge| {
            edge.kind == "calls"
                && edge.from == "auth::AuthService::login"
                && edge.to.contains("validate")
        }));
        assert!(
            details
                .nodes
                .iter()
                .any(|node| { node.id == "auth::AuthService::login" && node.kind == "method" })
        );
        assert!(details.nodes.iter().any(|node| {
            node.kind == "import" && node.id.contains("use crate::auth::{AuthService, Session};")
        }));
        assert!(
            details
                .nodes
                .iter()
                .any(|node| node.kind == "reference" && node.id == "Session")
        );
        assert!(details.symbol_index.iter().any(|entry| {
            entry.symbol == "login"
                && entry.node_ids == vec!["auth::AuthService::login".to_string()]
        }));
    }

    #[test]
    fn structure_analysis_supports_multiple_qualified_symbol_queries() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            r#"
mod auth {
    pub struct AuthService;

    impl AuthService {
        pub fn login(&self) {
            self.validate();
        }

        fn validate(&self) {}
    }
}
"#,
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        for (symbol, expected_kind, expected_signature) in [
            ("auth::AuthService::login", "method", "pub fn login(&self)"),
            (
                "auth::AuthService::validate",
                "method",
                "fn validate(&self)",
            ),
        ] {
            let response = analyze_project(
                tempdir.path(),
                &config,
                AnalysisRequest {
                    kind: AnalysisKind::Ast,
                    language: SupportedLanguage::Rust,
                    symbol: Some(symbol.to_string()),
                    path: None,
                    paths: Vec::new(),

                    line: None,
                },
            )
            .expect("analysis should succeed");

            let details = response.details.expect("details should exist");
            assert_eq!(details.symbol_query.as_deref(), Some(symbol));
            assert_eq!(details.units.len(), 1);
            assert_eq!(details.units[0].qualified_symbol.as_deref(), Some(symbol));
            assert_eq!(details.units[0].kind, expected_kind);
            assert_eq!(
                details.units[0].signature.as_deref(),
                Some(expected_signature)
            );
        }
    }

    #[test]
    fn context_analysis_uses_qualified_calls_to_reduce_false_incoming_edges() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            r#"
mod auth {
    pub fn validate() {}
}

mod audit {
    pub fn validate() {}
}

fn login() {
    crate::auth::validate();
}
"#,
        )
        .expect("fixture should write");
        let config = TldrConfig::for_project(tempdir.path().to_path_buf());

        let auth = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::CallGraph,
                language: SupportedLanguage::Rust,
                symbol: Some("auth::validate".to_string()),
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");
        assert!(auth.summary.contains("incoming [login]"));

        let audit = analyze_project(
            tempdir.path(),
            &config,
            AnalysisRequest {
                kind: AnalysisKind::CallGraph,
                language: SupportedLanguage::Rust,
                symbol: Some("audit::validate".to_string()),
                path: None,
                paths: Vec::new(),

                line: None,
            },
        )
        .expect("analysis should succeed");
        assert!(audit.summary.contains("incoming [none]"));
    }
}

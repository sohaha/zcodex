use crate::lang_support::LanguageRegistry;
use crate::lang_support::SupportedLanguage;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticConfig {
    pub enabled: bool,
    pub feature_gate: String,
    pub model: String,
    pub auto_reindex_threshold: usize,
}

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            feature_gate: "semantic-embed".to_string(),
            model: "minilm".to_string(),
            auto_reindex_threshold: 20,
        }
    }
}

impl SemanticConfig {
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SemanticReindexStatus {
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticReindexReport {
    pub status: SemanticReindexStatus,
    pub languages: Vec<SupportedLanguage>,
    pub indexed_files: usize,
    pub indexed_units: usize,
    pub truncated: bool,
    pub started_at: SystemTime,
    pub finished_at: SystemTime,
    pub message: String,
}

impl SemanticReindexReport {
    pub fn is_completed(&self) -> bool {
        matches!(self.status, SemanticReindexStatus::Completed)
    }

    pub fn completed(
        languages: Vec<SupportedLanguage>,
        indexed_files: usize,
        indexed_units: usize,
        started_at: SystemTime,
        finished_at: SystemTime,
    ) -> Self {
        Self {
            status: SemanticReindexStatus::Completed,
            languages,
            indexed_files,
            indexed_units,
            truncated: false,
            started_at,
            finished_at,
            message: format!(
                "semantic phase-1 reindex completed: {indexed_units} units across {indexed_files} files"
            ),
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        let now = SystemTime::now();
        Self {
            status: SemanticReindexStatus::Failed,
            languages: LanguageRegistry.supported_languages(),
            indexed_files: 0,
            indexed_units: 0,
            truncated: false,
            started_at: now,
            finished_at: now,
            message: format!("semantic phase-1 reindex failed: {}", error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticSearchRequest {
    pub language: SupportedLanguage,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmbeddingUnit {
    pub path: PathBuf,
    pub language: SupportedLanguage,
    pub symbol: Option<String>,
    pub kind: String,
    pub line: usize,
    pub code_preview: String,
    pub calls: Vec<String>,
    pub called_by: Vec<String>,
    pub dependencies: Vec<String>,
    pub cfg_summary: String,
    pub dfg_summary: String,
}

impl EmbeddingUnit {
    pub fn build_embedding_text(&self) -> String {
        [
            format!(
                "symbol={} kind={} file={} line={}",
                self.symbol.as_deref().unwrap_or("<file>"),
                self.kind,
                self.path.display(),
                self.line,
            ),
            format!("code: {}", self.code_preview),
            format!("calls: {}", join_or_none(&self.calls)),
            format!("called_by: {}", join_or_none(&self.called_by)),
            format!(
                "cfg: {}; dfg: {}; dependencies: {}",
                self.cfg_summary,
                self.dfg_summary,
                join_or_none(&self.dependencies)
            ),
        ]
        .join("\n")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticMatch {
    pub score: usize,
    pub path: PathBuf,
    pub line: usize,
    pub snippet: String,
    pub unit: EmbeddingUnit,
    pub embedding_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticSearchResponse {
    pub enabled: bool,
    pub query: String,
    pub indexed_files: usize,
    pub truncated: bool,
    pub matches: Vec<SemanticMatch>,
    pub message: String,
}

/// Minimal semantic indexer that ports the upstream embedding-unit shape and
/// five-layer text assembly without introducing heavyweight embedding deps yet.
#[derive(Debug, Clone)]
pub struct SemanticIndexer {
    config: SemanticConfig,
}

impl SemanticIndexer {
    pub fn new(config: SemanticConfig) -> Self {
        Self { config }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    pub fn auto_reindex_threshold(&self) -> usize {
        self.config.auto_reindex_threshold
    }

    pub fn should_reindex(&self, dirty_files: usize) -> bool {
        dirty_files >= self.config.auto_reindex_threshold
    }

    pub fn describe(&self) -> String {
        format!(
            "semantic {} threshold={}, feature_gate={}",
            if self.is_enabled() {
                "enabled"
            } else {
                "disabled"
            },
            self.config.auto_reindex_threshold,
            self.config.feature_gate
        )
    }

    pub fn reindex(&self, project_root: &Path) -> Result<SemanticReindexReport> {
        if !self.is_enabled() {
            return Ok(SemanticReindexReport::failed(
                "semantic reindexing is disabled in config",
            ));
        }
        let started_at = SystemTime::now();
        let registry = LanguageRegistry;
        let languages = registry.supported_languages();
        let mut indexed_files = 0;
        let mut indexed_units = 0;
        for language in &languages {
            let (units, files) = collect_embedding_units(project_root, *language)?;
            indexed_files += files;
            indexed_units += units.len();
        }
        let finished_at = SystemTime::now();
        Ok(SemanticReindexReport::completed(
            languages,
            indexed_files,
            indexed_units,
            started_at,
            finished_at,
        ))
    }

    pub fn search(
        &self,
        project_root: &Path,
        request: SemanticSearchRequest,
    ) -> Result<SemanticSearchResponse> {
        if !self.is_enabled() {
            return Ok(SemanticSearchResponse {
                enabled: false,
                query: request.query,
                indexed_files: 0,
                truncated: false,
                matches: Vec::new(),
                message:
                    "semantic search is disabled; enable [semantic].enabled in .codex/tldr.toml"
                        .to_string(),
            });
        }

        let (units, indexed_files) = collect_embedding_units(project_root, request.language)?;
        let query = request.query;
        let mut matches: Vec<_> = units
            .into_iter()
            .filter_map(|unit| {
                let embedding_text = unit.build_embedding_text();
                let score = score_match(&query, &unit, &embedding_text);
                if score == 0 {
                    return None;
                }
                let (line, snippet) = best_matching_line(&query, &unit);
                Some(SemanticMatch {
                    score,
                    path: unit.path.clone(),
                    line,
                    snippet,
                    unit,
                    embedding_text,
                })
            })
            .collect();
        matches.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.line.cmp(&right.line))
        });
        let truncated = matches.len() > 5;
        matches.truncate(5);
        let result_count = matches.len();

        Ok(SemanticSearchResponse {
            enabled: true,
            query,
            indexed_files,
            truncated,
            matches,
            message: format!("semantic search returned {result_count} matches"),
        })
    }
}

fn collect_embedding_units(
    project_root: &Path,
    language: SupportedLanguage,
) -> Result<(Vec<EmbeddingUnit>, usize)> {
    let mut files = Vec::new();
    collect_source_files(project_root, extension_for(language), &mut files)?;
    let indexed_files = files.len();

    let mut units = Vec::new();
    for path in files {
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        let relative_path = path
            .strip_prefix(project_root)
            .map(Path::to_path_buf)
            .unwrap_or(path.clone());
        let file_units = extract_units(&relative_path, language, &contents);
        if file_units.is_empty() {
            units.push(file_level_unit(relative_path, language, &contents));
        } else {
            units.extend(file_units);
        }
    }

    let symbol_index = build_called_by_index(&units);
    Ok((
        units
            .into_iter()
            .map(|mut unit| {
                unit.called_by = symbol_index
                    .get(unit.symbol.as_deref().unwrap_or_default())
                    .cloned()
                    .unwrap_or_default();
                unit
            })
            .collect(),
        indexed_files,
    ))
}

fn collect_source_files(root: &Path, extension: &str, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(".git" | "target" | "node_modules" | ".idea" | ".vscode")
            ) {
                continue;
            }
            collect_source_files(&path, extension, files)?;
            continue;
        }
        if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
    Ok(())
}

fn extract_units(path: &Path, language: SupportedLanguage, contents: &str) -> Vec<EmbeddingUnit> {
    let mut units = Vec::new();
    let mut block = Vec::new();
    let mut current_symbol: Option<String> = None;
    let mut current_kind: Option<&'static str> = None;
    let mut start_line = 1usize;

    for (index, line) in contents.lines().enumerate() {
        let line_number = index + 1;
        if let Some((symbol, kind)) = definition_for_line(language, line) {
            if let Some(symbol_name) = current_symbol.take() {
                units.push(build_unit(
                    path.to_path_buf(),
                    language,
                    Some(symbol_name),
                    current_kind.unwrap_or("symbol").to_string(),
                    start_line,
                    block.join("\n"),
                ));
                block.clear();
            }
            current_symbol = Some(symbol);
            current_kind = Some(kind);
            start_line = line_number;
        }
        if current_symbol.is_some() {
            block.push(line.trim().to_string());
            if block.len() >= 12
                && let Some(symbol_name) = current_symbol.take()
            {
                units.push(build_unit(
                    path.to_path_buf(),
                    language,
                    Some(symbol_name),
                    current_kind.unwrap_or("symbol").to_string(),
                    start_line,
                    block.join("\n"),
                ));
                block.clear();
            }
        }
    }

    if let Some(symbol_name) = current_symbol {
        units.push(build_unit(
            path.to_path_buf(),
            language,
            Some(symbol_name),
            current_kind.unwrap_or("symbol").to_string(),
            start_line,
            block.join("\n"),
        ));
    }

    units
}

fn file_level_unit(path: PathBuf, language: SupportedLanguage, contents: &str) -> EmbeddingUnit {
    build_unit(
        path,
        language,
        None,
        "file".to_string(),
        1,
        preview(contents, 8),
    )
}

fn build_unit(
    path: PathBuf,
    language: SupportedLanguage,
    symbol: Option<String>,
    kind: String,
    line: usize,
    code_preview: String,
) -> EmbeddingUnit {
    let calls = extract_calls(&code_preview, symbol.as_deref());
    EmbeddingUnit {
        dependencies: dependency_segments(&path),
        cfg_summary: format!(
            "{} lines sampled; {} outgoing calls",
            code_preview.lines().count(),
            calls.len()
        ),
        dfg_summary: if code_preview.contains("let ") || code_preview.contains("const ") {
            "contains local assignments".to_string()
        } else {
            "no obvious local assignments in preview".to_string()
        },
        path,
        language,
        symbol,
        kind,
        line,
        code_preview,
        called_by: Vec::new(),
        calls,
    }
}

fn build_called_by_index(units: &[EmbeddingUnit]) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for unit in units {
        let Some(caller) = unit.symbol.as_deref() else {
            continue;
        };
        for callee in &unit.calls {
            let called_by = index.entry(callee.clone()).or_default();
            if !called_by.iter().any(|existing| existing == caller) {
                called_by.push(caller.to_string());
            }
        }
    }
    index
}

fn definition_for_line(language: SupportedLanguage, line: &str) -> Option<(String, &'static str)> {
    let trimmed = line.trim();
    let candidates: &[(&str, &str)] = match language {
        SupportedLanguage::Rust => &[
            ("pub async fn ", "function"),
            ("async fn ", "function"),
            ("pub fn ", "function"),
            ("fn ", "function"),
            ("pub struct ", "struct"),
            ("struct ", "struct"),
            ("pub enum ", "enum"),
            ("enum ", "enum"),
            ("pub trait ", "trait"),
            ("trait ", "trait"),
        ],
        SupportedLanguage::TypeScript | SupportedLanguage::JavaScript => &[
            ("export async function ", "function"),
            ("async function ", "function"),
            ("export function ", "function"),
            ("function ", "function"),
            ("export class ", "class"),
            ("class ", "class"),
            ("interface ", "interface"),
            ("const ", "const"),
        ],
        SupportedLanguage::Python => &[("def ", "function"), ("class ", "class")],
        SupportedLanguage::Go => &[("func ", "function"), ("type ", "type")],
        SupportedLanguage::Php => &[
            ("function ", "function"),
            ("class ", "class"),
            ("interface ", "interface"),
        ],
        SupportedLanguage::Zig => &[("pub fn ", "function"), ("fn ", "function")],
    };

    candidates.iter().find_map(|(prefix, kind)| {
        trimmed
            .strip_prefix(prefix)
            .and_then(extract_symbol_name)
            .map(|symbol| (symbol, *kind))
    })
}

fn extract_symbol_name(rest: &str) -> Option<String> {
    let mut symbol = String::new();
    for ch in rest.chars() {
        if ch.is_alphanumeric() || ch == '_' {
            symbol.push(ch);
        } else {
            break;
        }
    }
    (!symbol.is_empty()).then_some(symbol)
}

fn extract_calls(code_preview: &str, current_symbol: Option<&str>) -> Vec<String> {
    let mut calls = Vec::new();
    for token in
        code_preview.split(|ch: char| ch.is_whitespace() || matches!(ch, '{' | '}' | ';' | ','))
    {
        let Some(name) = token.split('(').next() else {
            continue;
        };
        if !token.contains('(') || name.is_empty() || Some(name) == current_symbol {
            continue;
        }
        if matches!(name, "if" | "for" | "while" | "match" | "return" | "let") {
            continue;
        }
        if name.chars().all(|ch| ch.is_alphanumeric() || ch == '_')
            && !calls.iter().any(|existing| existing == name)
        {
            calls.push(name.to_string());
        }
    }
    calls
}

fn dependency_segments(path: &Path) -> Vec<String> {
    path.components()
        .filter_map(|component| {
            let value = component.as_os_str().to_str()?;
            (!value.is_empty() && value != ".").then_some(value.to_string())
        })
        .collect()
}

fn preview(contents: &str, max_lines: usize) -> String {
    contents
        .lines()
        .take(max_lines)
        .map(str::trim)
        .collect::<Vec<_>>()
        .join("\n")
}

fn best_matching_line(query: &str, unit: &EmbeddingUnit) -> (usize, String) {
    let tokens = tokenize(query);
    let mut best = (unit.line, String::new(), 0usize);
    for (offset, line) in unit.code_preview.lines().enumerate() {
        let score = tokens
            .iter()
            .filter(|token| line.to_ascii_lowercase().contains(token.as_str()))
            .count();
        if score > best.2 {
            best = (unit.line + offset, line.trim().to_string(), score);
        }
    }
    if best.1.is_empty() {
        let snippet = unit
            .code_preview
            .lines()
            .next()
            .map(|line| line.trim().to_string())
            .unwrap_or_default();
        (unit.line, snippet)
    } else {
        (best.0, best.1)
    }
}

fn score_match(query: &str, unit: &EmbeddingUnit, embedding_text: &str) -> usize {
    let tokens = tokenize(query);
    if tokens.is_empty() {
        return 0;
    }

    let symbol = unit
        .symbol
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = unit.path.display().to_string().to_ascii_lowercase();
    let preview = unit.code_preview.to_ascii_lowercase();
    let embedding = embedding_text.to_ascii_lowercase();

    tokens
        .iter()
        .map(|token| {
            let mut score = 0;
            if symbol.contains(token) {
                score += 5;
            }
            if path.contains(token) {
                score += 3;
            }
            if preview.contains(token) {
                score += 2;
            }
            if embedding.contains(token) {
                score += 1;
            }
            score
        })
        .sum()
}

fn tokenize(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn extension_for(language: SupportedLanguage) -> &'static str {
    match language {
        SupportedLanguage::Rust => "rs",
        SupportedLanguage::TypeScript => "ts",
        SupportedLanguage::JavaScript => "js",
        SupportedLanguage::Python => "py",
        SupportedLanguage::Go => "go",
        SupportedLanguage::Php => "php",
        SupportedLanguage::Zig => "zig",
    }
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::EmbeddingUnit;
    use super::SemanticConfig;
    use super::SemanticIndexer;
    use super::SemanticSearchRequest;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn semantic_indexer_defaults_disabled() {
        let config = SemanticConfig::default();
        let indexer = SemanticIndexer::new(config);

        assert!(!indexer.is_enabled());
        assert_eq!(indexer.auto_reindex_threshold(), 20);
        assert!(!indexer.should_reindex(19));
        assert!(indexer.should_reindex(20));
    }

    #[test]
    fn semantic_config_with_enabled_toggle() {
        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        assert!(indexer.is_enabled());
        assert!(indexer.describe().contains("enabled"));
    }

    #[test]
    fn embedding_text_uses_five_layers() {
        let text = EmbeddingUnit {
            path: "src/lib.rs".into(),
            language: SupportedLanguage::Rust,
            symbol: Some("login".to_string()),
            kind: "function".to_string(),
            line: 7,
            code_preview: "fn login() { validate(user); }".to_string(),
            calls: vec!["validate".to_string()],
            called_by: vec!["router".to_string()],
            dependencies: vec!["src".to_string(), "lib.rs".to_string()],
            cfg_summary: "1 lines sampled; 1 outgoing calls".to_string(),
            dfg_summary: "contains local assignments".to_string(),
        }
        .build_embedding_text();

        let expected = [
            "symbol=login kind=function file=src/lib.rs line=7",
            "code: fn login() { validate(user); }",
            "calls: validate",
            "called_by: router",
            "cfg: 1 lines sampled; 1 outgoing calls; dfg: contains local assignments; dependencies: src, lib.rs",
        ]
        .join("\n");
        assert_eq!(text, expected);
    }

    #[test]
    fn semantic_search_returns_ranked_matches() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        std::fs::write(
            tempdir.path().join("src/lib.rs"),
            "fn login() {\n    validate(user);\n}\nfn validate(user: &str) {\n    println!(\"{}\", user);\n}\n",
        )
        .expect("fixture should be written");

        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        let response = indexer
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "login validate".to_string(),
                },
            )
            .expect("search should succeed");

        assert_eq!(response.enabled, true);
        assert_eq!(response.query, "login validate");
        assert_eq!(response.indexed_files, 1);
        assert_eq!(response.truncated, false);
        assert_eq!(response.message, "semantic search returned 2 matches");
        assert_eq!(response.matches.len(), 2);
        assert_eq!(response.matches[0].unit.symbol.as_deref(), Some("login"));
        assert_eq!(response.matches[0].path, PathBuf::from("src/lib.rs"));
        assert_eq!(response.matches[0].line, 1);
        assert_eq!(response.matches[0].snippet, "fn login() {");
        assert_eq!(response.matches[0].unit.calls, vec!["validate".to_string()]);
        assert_eq!(
            response.matches[1].unit.called_by,
            vec!["login".to_string()]
        );
    }

    #[test]
    fn semantic_search_reports_disabled_gate() {
        let tempdir = tempdir().expect("tempdir should exist");
        let response = SemanticIndexer::new(SemanticConfig::default())
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "auth".to_string(),
                },
            )
            .expect("search should succeed");

        assert_eq!(response.enabled, false);
        assert_eq!(response.indexed_files, 0);
        assert_eq!(response.matches, Vec::new());
        assert_eq!(
            response.message,
            "semantic search is disabled; enable [semantic].enabled in .codex/tldr.toml"
        );
    }
}

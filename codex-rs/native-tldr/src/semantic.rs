mod embedder;

use crate::lang_support::LanguageRegistry;
use crate::lang_support::SupportedLanguage;
use crate::rust_analysis;
use crate::semantic_cache;
use anyhow::Context;
use anyhow::Result;
use embedder::SemanticEmbedder;
pub(crate) use embedder::onnx_runtime_status;
use ignore::gitignore::Gitignore;
use ignore::gitignore::GitignoreBuilder;
use serde::Deserialize;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
#[cfg(test)]
use std::sync::atomic::Ordering;
use std::time::SystemTime;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticEmbeddingConfig {
    pub enabled: bool,
    pub dimensions: usize,
}

impl Default for SemanticEmbeddingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dimensions: 64,
        }
    }
}

impl SemanticEmbeddingConfig {
    pub fn new(enabled: bool, dimensions: usize) -> Self {
        Self {
            enabled,
            dimensions,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticConfig {
    pub enabled: bool,
    pub feature_gate: String,
    pub model: String,
    pub auto_reindex_threshold: usize,
    pub embedding_enabled: bool,
    pub embedding: SemanticEmbeddingConfig,
    pub ignore: Vec<String>,
}

pub const SUPPORTED_SEMANTIC_MODELS: &[&str] = &[
    "minilm",
    "all-minilm-l6-v2",
    "bge-small-en-v1.5",
    "bge-base-en-v1.5",
    "bge-m3",
    "jina-code",
    "jina-embeddings-v2-base-code",
];
const SEMANTIC_EMBEDDING_BATCH_SIZE: usize = 64;

impl Default for SemanticConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            feature_gate: "semantic-embed".to_string(),
            model: "minilm".to_string(),
            auto_reindex_threshold: 20,
            embedding_enabled: SemanticEmbeddingConfig::default().enabled,
            embedding: SemanticEmbeddingConfig::default(),
            ignore: Vec::new(),
        }
    }
}

pub fn warm_embedding_model(model: &str, dimensions: usize) -> Result<()> {
    validate_semantic_model(model)?;
    let embedder = SemanticEmbedder::new(model.to_string());
    embedder
        .embed_query("ztldr semantic model warmup", dimensions)
        .with_context(|| format!("预热语义嵌入模型 `{model}`"))?;
    Ok(())
}

pub fn validate_semantic_model(model: &str) -> Result<()> {
    if SUPPORTED_SEMANTIC_MODELS.contains(&model) {
        return Ok(());
    }
    anyhow::bail!(
        "不支持语义嵌入模型 `{model}`；支持的模型：{}",
        SUPPORTED_SEMANTIC_MODELS.join(", ")
    );
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

    pub fn embedding_enabled(&self) -> bool {
        self.embedding_enabled
    }

    pub fn embedding_dimensions(&self) -> usize {
        self.embedding.dimensions
    }

    pub fn with_ignore(mut self, ignore: Vec<String>) -> Self {
        self.ignore = ignore;
        self
    }

    pub fn with_embedding(mut self, embedding: SemanticEmbeddingConfig) -> Self {
        self.embedding = embedding;
        self.embedding_enabled = self.embedding.enabled;
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
    pub embedding_enabled: bool,
    pub embedding_dimensions: usize,
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
        embedding_enabled: bool,
        embedding_dimensions: usize,
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
                "语义 phase-2 重新索引完成：{indexed_files} 个文件，共 {indexed_units} 个单元"
            ),
            embedding_enabled,
            embedding_dimensions,
        }
    }

    pub fn failed(
        languages: Vec<SupportedLanguage>,
        error: impl Into<String>,
        embedding_enabled: bool,
        embedding_dimensions: usize,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            status: SemanticReindexStatus::Failed,
            languages,
            indexed_files: 0,
            indexed_units: 0,
            truncated: false,
            started_at: now,
            finished_at: now,
            message: format!("语义 phase-2 重新索引失败：{}", error.into()),
            embedding_enabled,
            embedding_dimensions,
        }
    }

    pub fn skipped(
        languages: Vec<SupportedLanguage>,
        message: impl Into<String>,
        embedding_enabled: bool,
        embedding_dimensions: usize,
    ) -> Self {
        let now = SystemTime::now();
        Self {
            status: SemanticReindexStatus::Skipped,
            languages,
            indexed_files: 0,
            indexed_units: 0,
            truncated: false,
            started_at: now,
            finished_at: now,
            message: message.into(),
            embedding_enabled,
            embedding_dimensions,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticSearchRequest {
    pub language: SupportedLanguage,
    pub query: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EmbeddingUnit {
    pub path: PathBuf,
    pub language: SupportedLanguage,
    pub symbol: Option<String>,
    pub qualified_symbol: Option<String>,
    pub symbol_aliases: Vec<String>,
    pub kind: String,
    pub owner_symbol: Option<String>,
    pub owner_kind: Option<String>,
    pub implemented_trait: Option<String>,
    pub line: usize,
    pub span_end_line: usize,
    pub module_path: Vec<String>,
    pub visibility: Option<String>,
    pub signature: Option<String>,
    pub docs: Vec<String>,
    pub imports: Vec<String>,
    pub references: Vec<String>,
    pub code_preview: String,
    pub calls: Vec<String>,
    pub called_by: Vec<String>,
    pub dependencies: Vec<String>,
    pub cfg_summary: String,
    pub dfg_summary: String,
    pub embedding_vector: Option<Vec<f32>>,
}

impl EmbeddingUnit {
    pub fn build_embedding_text(&self) -> String {
        [
            format!(
                "symbol={} qualified={} aliases={} kind={} owner={} owner_kind={} implemented_trait={} file={} line={} end_line={}",
                self.symbol.as_deref().unwrap_or("<file>"),
                self.qualified_symbol.as_deref().unwrap_or("<none>"),
                join_or_none(&self.symbol_aliases),
                self.kind,
                self.owner_symbol.as_deref().unwrap_or("<none>"),
                self.owner_kind.as_deref().unwrap_or("<none>"),
                self.implemented_trait.as_deref().unwrap_or("<none>"),
                self.path.display(),
                self.line,
                self.span_end_line,
            ),
            format!(
                "module_path={} visibility={} signature={}",
                join_or_none(&self.module_path),
                self.visibility.as_deref().unwrap_or("<none>"),
                self.signature.as_deref().unwrap_or("<none>"),
            ),
            format!("docs: {}", join_or_none(&self.docs)),
            format!("imports: {}", join_or_none(&self.imports)),
            format!("references: {}", join_or_none(&self.references)),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticMatch {
    pub score: usize,
    pub path: PathBuf,
    pub line: usize,
    pub snippet: String,
    pub unit: EmbeddingUnit,
    pub embedding_text: String,
    pub embedding_score: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticSearchResponse {
    pub enabled: bool,
    pub query: String,
    pub indexed_files: usize,
    pub truncated: bool,
    pub matches: Vec<SemanticMatch>,
    pub embedding_used: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticIndex {
    pub language: SupportedLanguage,
    pub indexed_files: usize,
    pub units: Vec<EmbeddingUnit>,
    pub embedding_enabled: bool,
    pub embedding_dimensions: usize,
    pub source_fingerprint: String,
}

/// 带本地持久化缓存的语义索引器，用于缓存嵌入单元和向量。
#[derive(Debug, Clone)]
pub struct SemanticIndexer {
    config: SemanticConfig,
    embedder: SemanticEmbedder,
}

impl SemanticIndexer {
    pub fn new(config: SemanticConfig) -> Self {
        let embedder = SemanticEmbedder::new(config.model.clone());
        Self { config, embedder }
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
            "语义索引{} threshold={}, feature_gate={}",
            if self.is_enabled() {
                "已启用"
            } else {
                "已禁用"
            },
            self.config.auto_reindex_threshold,
            self.config.feature_gate
        )
    }

    fn build_ignore_matcher(&self, project_root: &Path) -> Result<Gitignore> {
        const DEFAULT_IGNORE: &[&str] =
            &[".git/", "target/", "node_modules/", ".idea/", ".vscode/"];

        let mut builder = GitignoreBuilder::new(project_root);
        for pattern in DEFAULT_IGNORE {
            builder
                .add_line(None, pattern)
                .with_context(|| format!("添加默认忽略模式 {pattern}"))?;
        }
        let ignore_file = project_root.join(".tldrignore");
        if ignore_file.exists() {
            builder.add(ignore_file);
        }
        for pattern in &self.config.ignore {
            let trimmed = pattern.trim();
            if trimmed.is_empty() {
                continue;
            }
            builder
                .add_line(None, trimmed)
                .with_context(|| format!("添加 tldr 忽略模式 {trimmed}"))?;
        }
        builder
            .build()
            .with_context(|| format!("为 {} 构建忽略匹配器", project_root.display()))
    }

    fn disabled_response(&self, query: String) -> SemanticSearchResponse {
        SemanticSearchResponse {
            enabled: false,
            query,
            indexed_files: 0,
            truncated: false,
            matches: Vec::new(),
            embedding_used: false,
            message: "语义搜索已禁用；请在 .codex/tldr.toml 中启用 [semantic].enabled".to_string(),
        }
    }

    pub fn load_or_build_index(
        &self,
        project_root: &Path,
        language: SupportedLanguage,
    ) -> Result<SemanticIndex> {
        let matcher = self.build_ignore_matcher(project_root)?;
        let mut files = Vec::new();
        collect_source_files(project_root, extensions_for(language), &mut files, &matcher)?;
        let source_fingerprint = semantic_cache::source_fingerprint(project_root, &files)?;
        if let Some(index) =
            semantic_cache::load_index(project_root, &self.config, language, &source_fingerprint)?
        {
            return Ok(index);
        }

        let index =
            self.build_index_from_files(project_root, language, files, source_fingerprint.clone())?;
        semantic_cache::persist_index(project_root, &self.config, &index, &source_fingerprint)?;
        Ok(index)
    }

    pub fn build_index(
        &self,
        project_root: &Path,
        language: SupportedLanguage,
    ) -> Result<SemanticIndex> {
        let matcher = self.build_ignore_matcher(project_root)?;
        let mut files = Vec::new();
        collect_source_files(project_root, extensions_for(language), &mut files, &matcher)?;
        let source_fingerprint = semantic_cache::source_fingerprint(project_root, &files)?;
        self.build_index_from_files(project_root, language, files, source_fingerprint)
    }

    pub(crate) fn build_symbol_structure_index(
        &self,
        project_root: &Path,
        language: SupportedLanguage,
        symbol: &str,
    ) -> Result<SemanticIndex> {
        let matcher = self.build_ignore_matcher(project_root)?;
        let mut files = Vec::new();
        collect_source_files(project_root, extensions_for(language), &mut files, &matcher)?;
        let indexed_files = files.len();
        let needles = symbol_needles(symbol);
        let mut units = Vec::new();

        for path in &files {
            let Ok(contents) = fs::read_to_string(path) else {
                continue;
            };
            if !needles.iter().any(|needle| contents.contains(needle)) {
                continue;
            }
            let relative_path = path
                .strip_prefix(project_root)
                .map(Path::to_path_buf)
                .unwrap_or_else(|_| path.clone());
            units.extend(
                extract_units(&relative_path, language, &contents)
                    .with_context(|| format!("从 {} 提取单元", relative_path.display()))?,
            );
        }

        Ok(SemanticIndex {
            language,
            indexed_files,
            units,
            embedding_enabled: false,
            embedding_dimensions: 0,
            source_fingerprint: format!("targeted-structure:{language:?}:{symbol}"),
        })
    }

    pub(crate) fn current_source_fingerprint(
        &self,
        project_root: &Path,
        language: SupportedLanguage,
    ) -> Result<String> {
        let matcher = self.build_ignore_matcher(project_root)?;
        let mut files = Vec::new();
        collect_source_files(project_root, extensions_for(language), &mut files, &matcher)?;
        semantic_cache::source_fingerprint(project_root, &files)
    }

    fn build_index_from_files(
        &self,
        project_root: &Path,
        language: SupportedLanguage,
        files: Vec<PathBuf>,
        source_fingerprint: String,
    ) -> Result<SemanticIndex> {
        let embedding_enabled = self.config.embedding_enabled();
        let embedding_dimensions = self.config.embedding_dimensions();
        let units = collect_embedding_units(
            project_root,
            language,
            &files,
            embedding_enabled,
            embedding_dimensions,
            &self.embedder,
        )?;

        #[cfg(test)]
        {
            SEMANTIC_INDEX_BUILD_COUNT.fetch_add(1, Ordering::SeqCst);
        }

        Ok(SemanticIndex {
            language,
            indexed_files: files.len(),
            units,
            embedding_enabled,
            embedding_dimensions,
            source_fingerprint,
        })
    }

    pub fn reindex_all(
        &self,
        project_root: &Path,
    ) -> Result<(Vec<SemanticIndex>, SemanticReindexReport)> {
        let registry = LanguageRegistry;
        let languages = registry.supported_languages();
        if !self.is_enabled() {
            return Ok((
                Vec::new(),
                SemanticReindexReport::failed(
                    languages,
                    "配置已禁用语义重新索引",
                    self.config.embedding_enabled(),
                    self.config.embedding_dimensions(),
                ),
            ));
        }
        let started_at = SystemTime::now();
        let mut indexes = Vec::with_capacity(languages.len());
        let mut indexed_files = 0;
        let mut indexed_units = 0;
        for language in &languages {
            let matcher = self.build_ignore_matcher(project_root)?;
            let mut files = Vec::new();
            collect_source_files(
                project_root,
                extensions_for(*language),
                &mut files,
                &matcher,
            )?;
            let source_fingerprint = semantic_cache::source_fingerprint(project_root, &files)?;
            let index = self.build_index_from_files(
                project_root,
                *language,
                files,
                source_fingerprint.clone(),
            )?;
            semantic_cache::persist_index(project_root, &self.config, &index, &source_fingerprint)?;
            indexed_files += index.indexed_files;
            indexed_units += index.units.len();
            indexes.push(index);
        }
        let finished_at = SystemTime::now();
        Ok((
            indexes,
            SemanticReindexReport::completed(
                languages,
                indexed_files,
                indexed_units,
                started_at,
                finished_at,
                self.config.embedding_enabled(),
                self.config.embedding_dimensions(),
            ),
        ))
    }

    pub fn reindex(&self, project_root: &Path) -> Result<SemanticReindexReport> {
        self.reindex_all(project_root).map(|(_, report)| report)
    }

    pub fn project_languages(&self, project_root: &Path) -> Result<Vec<SupportedLanguage>> {
        let matcher = self.build_ignore_matcher(project_root)?;
        let registry = LanguageRegistry;
        let mut languages = Vec::new();
        for language in registry.supported_languages() {
            let mut files = Vec::new();
            collect_source_files(project_root, extensions_for(language), &mut files, &matcher)?;
            if !files.is_empty() {
                languages.push(language);
            }
        }
        Ok(languages)
    }

    pub fn reindex_languages(
        &self,
        project_root: &Path,
        languages: &[SupportedLanguage],
    ) -> Result<(Vec<SemanticIndex>, SemanticReindexReport)> {
        if !self.is_enabled() {
            return Ok((
                Vec::new(),
                SemanticReindexReport::failed(
                    languages.to_vec(),
                    "配置已禁用语义重新索引",
                    self.config.embedding_enabled(),
                    self.config.embedding_dimensions(),
                ),
            ));
        }
        if languages.is_empty() {
            return Ok((
                Vec::new(),
                SemanticReindexReport::skipped(
                    Vec::new(),
                    "没有标记为脏的语义源文件",
                    self.config.embedding_enabled(),
                    self.config.embedding_dimensions(),
                ),
            ));
        }
        let matcher = self.build_ignore_matcher(project_root)?;
        let started_at = SystemTime::now();
        let mut indexes = Vec::with_capacity(languages.len());
        let mut indexed_files = 0;
        let mut indexed_units = 0;
        for language in languages {
            let mut files = Vec::new();
            collect_source_files(
                project_root,
                extensions_for(*language),
                &mut files,
                &matcher,
            )?;
            let source_fingerprint = semantic_cache::source_fingerprint(project_root, &files)?;
            let index = self.build_index_from_files(
                project_root,
                *language,
                files,
                source_fingerprint.clone(),
            )?;
            semantic_cache::persist_index(project_root, &self.config, &index, &source_fingerprint)?;
            indexed_files += index.indexed_files;
            indexed_units += index.units.len();
            indexes.push(index);
        }
        let finished_at = SystemTime::now();
        Ok((
            indexes,
            SemanticReindexReport::completed(
                languages.to_vec(),
                indexed_files,
                indexed_units,
                started_at,
                finished_at,
                self.config.embedding_enabled(),
                self.config.embedding_dimensions(),
            ),
        ))
    }

    pub fn search_index(
        &self,
        index: &SemanticIndex,
        query: String,
    ) -> Result<SemanticSearchResponse> {
        let (query_vector, embedding_used) = if index.embedding_enabled {
            match self
                .embedder
                .embed_query(&query, index.embedding_dimensions)
            {
                Ok(vector) => (Some(vector), true),
                Err(error) if embedder::is_embedding_backend_unavailable(&error) => (None, false),
                Err(error) => return Err(error).context("嵌入语义搜索查询"),
            }
        } else {
            (None, false)
        };
        let mut matches: Vec<_> = index
            .units
            .iter()
            .cloned()
            .map(|unit| {
                let embedding_text = unit.build_embedding_text();
                let score = score_match(&query, &unit, &embedding_text);
                let (line, snippet) = best_matching_line(&query, &unit);
                let embedding_score = query_vector.as_ref().and_then(|query_vec| {
                    unit.embedding_vector
                        .as_deref()
                        .map(|unit_vec| dot_product(query_vec, unit_vec))
                });
                SemanticMatch {
                    score,
                    path: unit.path.clone(),
                    line,
                    snippet,
                    unit,
                    embedding_text,
                    embedding_score,
                }
            })
            .filter(|semantic_match| {
                semantic_match.score > 0 || semantic_match.embedding_score.unwrap_or_default() > 0.0
            })
            .collect();
        matches.sort_by(|left, right| {
            right
                .embedding_score
                .unwrap_or_default()
                .total_cmp(&left.embedding_score.unwrap_or_default())
                .then_with(|| right.score.cmp(&left.score))
                .then_with(|| left.path.cmp(&right.path))
                .then_with(|| left.line.cmp(&right.line))
        });
        let truncated = matches.len() > 5;
        matches.truncate(5);
        let result_count = matches.len();

        Ok(SemanticSearchResponse {
            enabled: true,
            query,
            indexed_files: index.indexed_files,
            truncated,
            matches,
            embedding_used,
            message: format!("语义搜索返回 {result_count} 个匹配项"),
        })
    }

    pub fn search(
        &self,
        project_root: &Path,
        request: SemanticSearchRequest,
    ) -> Result<SemanticSearchResponse> {
        if !self.is_enabled() {
            return Ok(self.disabled_response(request.query));
        }

        let index = self.load_or_build_index(project_root, request.language)?;
        self.search_index(&index, request.query)
    }
}

#[cfg(test)]
pub(crate) static SEMANTIC_INDEX_BUILD_COUNT: AtomicUsize = AtomicUsize::new(0);

#[cfg(test)]
pub(crate) fn reset_semantic_index_build_count() {
    SEMANTIC_INDEX_BUILD_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(test)]
pub(crate) fn semantic_index_build_count() -> usize {
    SEMANTIC_INDEX_BUILD_COUNT.load(Ordering::SeqCst)
}

fn collect_embedding_units(
    project_root: &Path,
    language: SupportedLanguage,
    files: &[PathBuf],
    embedding_enabled: bool,
    embedding_dims: usize,
    embedder: &SemanticEmbedder,
) -> Result<Vec<EmbeddingUnit>> {
    let mut units = Vec::new();
    for path in files {
        let Ok(contents) = fs::read_to_string(path) else {
            continue;
        };
        let relative_path = path
            .strip_prefix(project_root)
            .map(Path::to_path_buf)
            .unwrap_or(path.clone());
        let file_units = extract_units(&relative_path, language, &contents)
            .with_context(|| format!("从 {} 提取单元", relative_path.display()))?;
        if file_units.is_empty() {
            units.push(file_level_unit(relative_path, language, &contents));
        } else {
            units.extend(file_units);
        }
    }

    let symbol_index = build_called_by_index(&units);
    for unit in &mut units {
        let mut called_by = Vec::new();
        for key in symbol_lookup_keys(unit) {
            if let Some(callers) = symbol_index.get(key.as_str()) {
                for caller in callers {
                    if !called_by.iter().any(|existing| existing == caller) {
                        called_by.push(caller.clone());
                    }
                }
            }
        }
        unit.called_by = called_by;
    }

    attach_embedding_vectors(&mut units, embedding_enabled, embedding_dims, embedder);
    Ok(units)
}

fn collect_source_files(
    root: &Path,
    extensions: &[&str],
    files: &mut Vec<PathBuf>,
    matcher: &Gitignore,
) -> Result<()> {
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
            let matched = matcher.matched(&path, true);
            if matched.is_ignore() && !matched.is_whitelist() {
                continue;
            }
            collect_source_files(&path, extensions, files, matcher)?;
            continue;
        }
        let matched = matcher.matched(&path, false);
        if matched.is_ignore() && !matched.is_whitelist() {
            continue;
        }
        if let Some(extension) = path.extension().and_then(|value| value.to_str())
            && extensions.iter().any(|candidate| candidate == &extension)
        {
            files.push(path);
        }
    }
    Ok(())
}

fn symbol_needles(symbol: &str) -> Vec<&str> {
    let trimmed = symbol.trim();
    let mut needles = Vec::new();
    if !trimmed.is_empty() {
        needles.push(trimmed);
    }
    if let Some(short_name) = trimmed.rsplit("::").next()
        && !short_name.is_empty()
        && !needles.contains(&short_name)
    {
        needles.push(short_name);
    }
    needles
}

fn extract_units(
    path: &Path,
    language: SupportedLanguage,
    contents: &str,
) -> Result<Vec<EmbeddingUnit>> {
    if LanguageRegistry::support_for(language)
        .symbol_extractor
        .uses_dedicated_extractor()
    {
        return rust_analysis::extract_units(path, contents);
    }
    Ok(extract_units_fallback(path, language, contents))
}

fn extract_units_fallback(
    path: &Path,
    language: SupportedLanguage,
    contents: &str,
) -> Vec<EmbeddingUnit> {
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
    let dependencies = dependency_segments(&path);
    let module_path = file_module_path(&path);
    let symbol_aliases = symbol.iter().cloned().collect();
    EmbeddingUnit {
        dependencies,
        cfg_summary: format!(
            "采样 {} 行；{} 个出站调用",
            code_preview.lines().count(),
            calls.len()
        ),
        dfg_summary: if code_preview.contains("let ") || code_preview.contains("const ") {
            "包含局部赋值".to_string()
        } else {
            "预览中没有明显的局部赋值".to_string()
        },
        path,
        language,
        symbol,
        qualified_symbol: None,
        symbol_aliases,
        kind,
        owner_symbol: None,
        owner_kind: None,
        implemented_trait: None,
        line,
        span_end_line: line + code_preview.lines().count().saturating_sub(1),
        module_path,
        visibility: None,
        signature: None,
        docs: Vec::new(),
        imports: Vec::new(),
        references: Vec::new(),
        code_preview,
        called_by: Vec::new(),
        calls,
        embedding_vector: None,
    }
}

fn build_called_by_index(units: &[EmbeddingUnit]) -> BTreeMap<String, Vec<String>> {
    let mut index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for unit in units {
        let Some(caller) = canonical_symbol(unit) else {
            continue;
        };
        for callee in &unit.calls {
            for key in call_lookup_keys(callee) {
                let called_by = index.entry(key).or_default();
                if !called_by.iter().any(|existing| existing == caller) {
                    called_by.push(caller.to_owned());
                }
            }
        }
    }
    index
}

fn call_lookup_keys(callee: &str) -> Vec<String> {
    let trimmed = callee.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let mut keys = vec![trimmed.to_string()];
    let stripped = trim_rust_path_prefixes(trimmed);
    if stripped != trimmed && !keys.iter().any(|existing| existing == stripped) {
        keys.push(stripped.to_string());
    }
    keys
}

fn trim_rust_path_prefixes(symbol: &str) -> &str {
    let mut trimmed = symbol.trim_start_matches("::");
    loop {
        if let Some(rest) = trimmed.strip_prefix("crate::") {
            trimmed = rest;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("self::") {
            trimmed = rest;
            continue;
        }
        if let Some(rest) = trimmed.strip_prefix("super::") {
            trimmed = rest;
            continue;
        }
        break;
    }
    trimmed
}

fn canonical_symbol(unit: &EmbeddingUnit) -> Option<&str> {
    unit.qualified_symbol
        .as_deref()
        .or(unit.symbol.as_deref())
        .or_else(|| unit.symbol_aliases.first().map(String::as_str))
}

fn symbol_lookup_keys(unit: &EmbeddingUnit) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(symbol) = &unit.symbol {
        keys.push(symbol.clone());
    }
    if let Some(qualified) = &unit.qualified_symbol
        && !keys.iter().any(|existing| existing == qualified)
    {
        keys.push(qualified.clone());
    }
    for alias in &unit.symbol_aliases {
        if !keys.iter().any(|existing| existing == alias) {
            keys.push(alias.clone());
        }
    }
    keys
}

fn definition_for_line(language: SupportedLanguage, line: &str) -> Option<(String, &'static str)> {
    let trimmed = line.trim();
    let candidates: &[(&str, &str)] = match language {
        SupportedLanguage::C | SupportedLanguage::Cpp => &[
            ("static inline ", "function"),
            ("inline ", "function"),
            ("class ", "class"),
            ("struct ", "struct"),
        ],
        SupportedLanguage::CSharp => &[
            ("public class ", "class"),
            ("class ", "class"),
            ("public interface ", "interface"),
            ("interface ", "interface"),
            ("public void ", "method"),
            ("void ", "method"),
        ],
        SupportedLanguage::Java => &[
            ("public class ", "class"),
            ("class ", "class"),
            ("public interface ", "interface"),
            ("interface ", "interface"),
        ],
        SupportedLanguage::Kotlin => &[
            ("fun ", "function"),
            ("class ", "class"),
            ("interface ", "interface"),
            ("object ", "object"),
        ],
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
        SupportedLanguage::Lua | SupportedLanguage::Luau => {
            &[("local function ", "function"), ("function ", "function")]
        }
        SupportedLanguage::Php => &[
            ("function ", "function"),
            ("class ", "class"),
            ("interface ", "interface"),
        ],
        SupportedLanguage::Ruby => &[("def ", "function"), ("class ", "class")],
        SupportedLanguage::Swift => &[("func ", "function"), ("class ", "class")],
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

fn file_module_path(path: &Path) -> Vec<String> {
    let mut segments = path
        .components()
        .filter_map(|component| component.as_os_str().to_str().map(str::to_string))
        .collect::<Vec<_>>();
    if matches!(segments.first().map(String::as_str), Some("src")) {
        segments.remove(0);
    }
    let Some(last) = segments.pop() else {
        return Vec::new();
    };
    let stem = Path::new(&last)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !matches!(stem, "" | "lib" | "main" | "mod") {
        segments.push(stem.to_string());
    }
    segments
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
    let qualified = unit
        .qualified_symbol
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let aliases = unit
        .symbol_aliases
        .iter()
        .map(|alias| alias.to_ascii_lowercase())
        .collect::<Vec<_>>();
    let signature = unit
        .signature
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let docs = unit.docs.join(" ").to_ascii_lowercase();
    let imports = unit.imports.join(" ").to_ascii_lowercase();
    let references = unit.references.join(" ").to_ascii_lowercase();
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
            if qualified.contains(token) {
                score += 8;
            }
            if aliases.iter().any(|alias| alias.contains(token)) {
                score += 6;
            }
            if signature.contains(token) {
                score += 4;
            }
            if references.contains(token) {
                score += 3;
            }
            if imports.contains(token) {
                score += 2;
            }
            if path.contains(token) {
                score += 3;
            }
            if preview.contains(token) {
                score += 2;
            }
            if docs.contains(token) {
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

fn attach_embedding_vectors(
    units: &mut [EmbeddingUnit],
    enabled: bool,
    dims: usize,
    embedder: &SemanticEmbedder,
) {
    if !enabled || dims == 0 {
        return;
    }

    for chunk in units.chunks_mut(SEMANTIC_EMBEDDING_BATCH_SIZE) {
        let texts = chunk
            .iter()
            .map(EmbeddingUnit::build_embedding_text)
            .collect::<Vec<_>>();
        let Ok(vectors) = embedder.embed_documents(&texts, dims) else {
            continue;
        };
        for (unit, vector) in chunk.iter_mut().zip(vectors) {
            if vector.iter().any(|&value| value != 0.0) {
                unit.embedding_vector = Some(vector);
            }
        }
    }
}

fn dot_product(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right.iter()).map(|(a, b)| a * b).sum()
}

fn extensions_for(language: SupportedLanguage) -> &'static [&'static str] {
    match language {
        SupportedLanguage::C => &["c"],
        SupportedLanguage::Cpp => &["cpp"],
        SupportedLanguage::CSharp => &["cs"],
        SupportedLanguage::Java => &["java"],
        SupportedLanguage::Kotlin => &["kt"],
        SupportedLanguage::Rust => &["rs"],
        SupportedLanguage::TypeScript => &["ts", "tsx"],
        SupportedLanguage::JavaScript => &["js", "jsx", "mjs", "cjs"],
        SupportedLanguage::Lua => &["lua"],
        SupportedLanguage::Luau => &["luau"],
        SupportedLanguage::Python => &["py"],
        SupportedLanguage::Go => &["go"],
        SupportedLanguage::Php => &["php"],
        SupportedLanguage::Ruby => &["rb"],
        SupportedLanguage::Swift => &["swift"],
        SupportedLanguage::Zig => &["zig"],
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
    use super::SemanticEmbeddingConfig;
    use super::SemanticIndexer;
    use super::SemanticSearchRequest;
    use super::embedder::reset_test_embedding_call_count;
    use super::embedder::set_test_embedding_failure;
    use super::embedder::test_embedding_call_count;
    use super::reset_semantic_index_build_count;
    use super::semantic_index_build_count;
    use super::validate_semantic_model;
    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use serial_test::serial;
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use tempfile::tempdir;

    #[test]
    fn semantic_indexer_defaults_enabled() {
        let config = SemanticConfig::default();
        let indexer = SemanticIndexer::new(config);

        assert!(indexer.is_enabled());
        assert_eq!(indexer.auto_reindex_threshold(), 20);
        assert!(!indexer.should_reindex(19));
        assert!(indexer.should_reindex(20));
    }

    #[test]
    fn semantic_config_with_enabled_toggle() {
        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        assert!(indexer.is_enabled());
        assert!(indexer.describe().contains("已启用"));
    }

    #[test]
    fn semantic_model_validation_accepts_supported_aliases() {
        validate_semantic_model("bge-m3").expect("default model should be supported");
        validate_semantic_model("jina-code").expect("alias should be supported");
    }

    #[test]
    fn semantic_model_validation_rejects_unknown_models() {
        let error =
            validate_semantic_model("unknown-model").expect_err("unknown model should fail");

        assert!(error.to_string().contains("不支持语义嵌入模型"));
        assert!(error.to_string().contains("bge-m3"));
    }

    #[test]
    fn embedding_text_uses_five_layers() {
        let text = EmbeddingUnit {
            path: "src/lib.rs".into(),
            language: SupportedLanguage::Rust,
            symbol: Some("login".to_string()),
            qualified_symbol: Some("auth::login".to_string()),
            symbol_aliases: vec!["login".to_string(), "auth::login".to_string()],
            kind: "function".to_string(),
            owner_symbol: None,
            owner_kind: None,
            implemented_trait: None,
            line: 7,
            span_end_line: 11,
            module_path: vec!["auth".to_string()],
            visibility: Some("pub".to_string()),
            signature: Some("pub fn login(token: &str) -> bool".to_string()),
            docs: vec!["Login entry point".to_string()],
            imports: vec!["use crate::auth::token;".to_string()],
            references: vec!["Token".to_string()],
            code_preview: "fn login() { validate(user); }".to_string(),
            calls: vec!["validate".to_string()],
            called_by: vec!["router".to_string()],
            dependencies: vec!["src".to_string(), "lib.rs".to_string()],
            cfg_summary: "采样 1 行；1 个出站调用".to_string(),
            dfg_summary: "包含局部赋值".to_string(),
            embedding_vector: None,
        }
        .build_embedding_text();

        let expected = [
            "symbol=login qualified=auth::login aliases=login, auth::login kind=function owner=<none> owner_kind=<none> implemented_trait=<none> file=src/lib.rs line=7 end_line=11",
            "module_path=auth visibility=pub signature=pub fn login(token: &str) -> bool",
            "docs: Login entry point",
            "imports: use crate::auth::token;",
            "references: Token",
            "code: fn login() { validate(user); }",
            "calls: validate",
            "called_by: router",
            "cfg: 采样 1 行；1 个出站调用; dfg: 包含局部赋值; dependencies: src, lib.rs",
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
        assert_eq!(response.message, "语义搜索返回 2 个匹配项");
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
    #[serial]
    fn semantic_index_batches_document_embedding_generation() {
        let tempdir = tempdir().expect("tempdir should exist");
        std::fs::create_dir_all(tempdir.path().join("src")).expect("src dir should exist");
        let source = (0..70)
            .map(|index| format!("fn handler_{index}() {{}}\n"))
            .collect::<String>();
        std::fs::write(tempdir.path().join("src/lib.rs"), source)
            .expect("fixture should be written");
        reset_test_embedding_call_count();

        let index = SemanticIndexer::new(SemanticConfig::default().with_enabled(true))
            .build_index(tempdir.path(), SupportedLanguage::Rust)
            .expect("index should build");

        assert_eq!(index.units.len(), 70);
        assert_eq!(test_embedding_call_count(), 2);
        assert!(index.units.iter().all(|unit| {
            unit.embedding_vector
                .as_ref()
                .is_some_and(|vector| vector.len() == 64)
        }));
    }

    #[test]
    fn semantic_search_reports_disabled_gate() {
        let tempdir = tempdir().expect("tempdir should exist");
        let response = SemanticIndexer::new(SemanticConfig::default().with_enabled(false))
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
            "语义搜索已禁用；请在 .codex/tldr.toml 中启用 [semantic].enabled"
        );
    }

    #[test]
    fn tldrignore_filters_cross_process_files() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(tempdir.path().join(".tldrignore"), "src/ignored.rs\n")
            .expect("tldrignore should write");
        std::fs::write(src.join("kept.rs"), "fn kept() {}\n").expect("kept file should write");
        std::fs::write(src.join("ignored.rs"), "fn skip() {}\n")
            .expect("ignored file should write");

        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        let report = indexer
            .reindex(tempdir.path())
            .expect("reindex should succeed");

        assert!(report.is_completed());
        assert_eq!(report.indexed_files, 1);
    }

    #[test]
    fn semantic_typescript_index_includes_tsx_files() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(
            src.join("App.tsx"),
            "export function App() {\n  return <main>ok</main>;\n}\n",
        )
        .expect("tsx fixture should write");

        let index = SemanticIndexer::new(SemanticConfig::default().with_enabled(true))
            .build_index(tempdir.path(), SupportedLanguage::TypeScript)
            .expect("typescript index should build");

        assert_eq!(index.indexed_files, 1);
        let paths: BTreeSet<_> = index.units.iter().map(|unit| unit.path.clone()).collect();
        assert_eq!(paths, BTreeSet::from([PathBuf::from("src/App.tsx")]));
    }

    #[test]
    fn semantic_javascript_index_includes_jsx_files() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(
            src.join("App.jsx"),
            "export function App() {\n  return <main>ok</main>;\n}\n",
        )
        .expect("jsx fixture should write");

        let index = SemanticIndexer::new(SemanticConfig::default().with_enabled(true))
            .build_index(tempdir.path(), SupportedLanguage::JavaScript)
            .expect("javascript index should build");

        assert_eq!(index.indexed_files, 1);
        let paths: BTreeSet<_> = index.units.iter().map(|unit| unit.path.clone()).collect();
        assert_eq!(paths, BTreeSet::from([PathBuf::from("src/App.jsx")]));
    }

    #[test]
    fn embedding_vectors_populated_when_enabled() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(
            src.join("vector.rs"),
            r#"
fn handle_request() {
    log();
}

fn log() {
    println!("ready");
}
"#,
        )
        .expect("vector fixture should write");

        let config = SemanticConfig::default()
            .with_enabled(true)
            .with_embedding(SemanticEmbeddingConfig::new(true, 16));
        let indexer = SemanticIndexer::new(config);
        let response = indexer
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "handle_request".to_string(),
                },
            )
            .expect("search should succeed");

        assert!(response.embedding_used);
        assert!(!response.matches.is_empty());
        let first = &response.matches[0];
        let vector = first
            .unit
            .embedding_vector
            .as_ref()
            .expect("embedding vector should be generated");
        assert!(!vector.is_empty());
        let score = first
            .embedding_score
            .expect("embedding score should be available");
        assert!(score > 0.0);
    }

    #[test]
    #[serial]
    fn semantic_search_auto_falls_back_when_ort_backend_is_unavailable() {
        set_test_embedding_failure(Some(
            "semantic embedding backend requires ONNX Runtime dylib `libonnxruntime.so` to be loadable: missing",
        ));

        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(
            src.join("vector.rs"),
            r#"
fn handle_request() {
    log();
}

fn log() {
    println!("ready");
}
"#,
        )
        .expect("vector fixture should write");

        let config = SemanticConfig::default()
            .with_enabled(true)
            .with_embedding(SemanticEmbeddingConfig::new(true, 16));
        let indexer = SemanticIndexer::new(config);
        let response = indexer
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "handle_request".to_string(),
                },
            )
            .expect("search should fall back instead of erroring");

        set_test_embedding_failure(None);

        assert_eq!(response.embedding_used, false);
        assert!(!response.matches.is_empty());
        assert_eq!(response.message, "语义搜索返回 2 个匹配项");
    }

    #[test]
    #[serial]
    fn semantic_search_preserves_non_ort_embedding_errors() {
        set_test_embedding_failure(Some("unexpected embedding failure"));

        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(src.join("vector.rs"), "fn handle_request() {}\n")
            .expect("vector fixture should write");

        let config = SemanticConfig::default()
            .with_enabled(true)
            .with_embedding(SemanticEmbeddingConfig::new(true, 16));
        let indexer = SemanticIndexer::new(config);
        let error = indexer
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "handle_request".to_string(),
                },
            )
            .expect_err("non-ORT embedding failures should still surface");

        set_test_embedding_failure(None);

        assert!(
            error
                .chain()
                .any(|cause| cause.to_string().contains("unexpected embedding failure"))
        );
    }

    #[test]
    fn semantic_search_ranks_qualified_symbol_query_first() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(
            src.join("lib.rs"),
            r#"
mod auth {
    pub struct AuthService;

    impl AuthService {
        pub fn login(&self, token: &str) -> bool {
            self.validate(token)
        }

        fn validate(&self, token: &str) -> bool {
            !token.is_empty()
        }
    }
}
"#,
        )
        .expect("fixture should write");
        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        let response = indexer
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "auth::AuthService::login token".to_string(),
                },
            )
            .expect("search should succeed");

        assert!(response.matches.len() >= 2);
        assert_eq!(
            response.matches[0].unit.qualified_symbol.as_deref(),
            Some("auth::AuthService::login")
        );
        assert!(response.matches[0].score >= response.matches[1].score);
    }

    #[test]
    fn semantic_search_called_by_uses_scoped_paths_for_disambiguation() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(
            src.join("lib.rs"),
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
        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));

        let auth = indexer
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "auth::validate".to_string(),
                },
            )
            .expect("auth search should succeed");
        let auth_validate = auth
            .matches
            .iter()
            .find(|item| item.unit.qualified_symbol.as_deref() == Some("auth::validate"))
            .expect("auth::validate should be indexed");
        assert!(auth_validate.unit.called_by.contains(&"login".to_string()));

        let audit = indexer
            .search(
                tempdir.path(),
                SemanticSearchRequest {
                    language: SupportedLanguage::Rust,
                    query: "audit::validate".to_string(),
                },
            )
            .expect("audit search should succeed");
        let audit_validate = audit
            .matches
            .iter()
            .find(|item| item.unit.qualified_symbol.as_deref() == Some("audit::validate"))
            .expect("audit::validate should be indexed");
        assert_eq!(audit_validate.unit.called_by, Vec::<String>::new());
    }

    #[test]
    #[serial]
    fn load_or_build_index_persists_and_reuses_disk_cache() {
        reset_semantic_index_build_count();
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(src.join("lib.rs"), "fn login() {}\n").expect("fixture should write");

        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        let first = indexer
            .load_or_build_index(tempdir.path(), SupportedLanguage::Rust)
            .expect("initial index build should succeed");
        assert_eq!(first.units[0].symbol.as_deref(), Some("login"));
        assert_eq!(semantic_index_build_count(), 1);
        let cache_dir = crate::daemon::daemon_artifact_dir_for_project(tempdir.path())
            .join("cache")
            .join("semantic")
            .join("rust");
        assert!(cache_dir.join("manifest.json").exists());
        assert!(cache_dir.join("units.jsonl").exists());
        assert!(cache_dir.join("vectors.f32").exists());

        let reused = SemanticIndexer::new(SemanticConfig::default().with_enabled(true))
            .load_or_build_index(tempdir.path(), SupportedLanguage::Rust)
            .expect("disk-backed index should load");
        assert_eq!(reused.units[0].symbol.as_deref(), Some("login"));
        assert_eq!(semantic_index_build_count(), 1);

        std::fs::write(src.join("lib.rs"), "fn logout() {}\n").expect("fixture should update");
        let refreshed = indexer
            .load_or_build_index(tempdir.path(), SupportedLanguage::Rust)
            .expect("changed source should rebuild");
        assert_eq!(refreshed.units[0].symbol.as_deref(), Some("logout"));
        assert_eq!(semantic_index_build_count(), 2);
    }

    #[test]
    fn reindex_refreshes_disk_cache() {
        let tempdir = tempdir().expect("tempdir should exist");
        let src = tempdir.path().join("src");
        std::fs::create_dir_all(&src).expect("src dir should exist");
        std::fs::write(src.join("lib.rs"), "fn login() {}\n").expect("fixture should write");

        let indexer = SemanticIndexer::new(SemanticConfig::default().with_enabled(true));
        let first = indexer
            .load_or_build_index(tempdir.path(), SupportedLanguage::Rust)
            .expect("initial index should build");
        assert_eq!(first.units[0].symbol.as_deref(), Some("login"));

        std::fs::write(src.join("lib.rs"), "fn logout() {}\n").expect("fixture should update");
        let report = indexer
            .reindex(tempdir.path())
            .expect("reindex should succeed");
        assert!(report.is_completed());
        assert!(report.message.contains("phase-2"));

        let refreshed = indexer
            .load_or_build_index(tempdir.path(), SupportedLanguage::Rust)
            .expect("refreshed index should load");
        assert_eq!(refreshed.units[0].symbol.as_deref(), Some("logout"));
    }
}

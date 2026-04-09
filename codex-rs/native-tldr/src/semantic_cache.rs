use crate::daemon::daemon_artifact_dir_for_project;
use crate::daemon::temp_artifact_dir_for_project;
use crate::lang_support::SupportedLanguage;
use crate::semantic::EmbeddingUnit;
use crate::semantic::SemanticConfig;
use crate::semantic::SemanticIndex;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const CACHE_VERSION: u32 = 1;
const MANIFEST_FILE: &str = "manifest.json";
const UNITS_FILE: &str = "units.jsonl";
const VECTORS_FILE: &str = "vectors.f32";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct SemanticIndexManifest {
    version: u32,
    language: SupportedLanguage,
    model: String,
    source_fingerprint: String,
    embedding_enabled: bool,
    embedding_dimensions: usize,
    indexed_files: usize,
    unit_count: usize,
    generated_at_unix_secs: u64,
}

pub(crate) fn load_index(
    project_root: &Path,
    config: &SemanticConfig,
    language: SupportedLanguage,
    source_fingerprint: &str,
) -> Result<Option<SemanticIndex>> {
    for cache_dir in cache_dir_candidates(project_root, language) {
        let manifest_path = cache_dir.join(MANIFEST_FILE);
        if !manifest_path.exists() {
            continue;
        }

        let manifest: SemanticIndexManifest = serde_json::from_str(
            &fs::read_to_string(&manifest_path)
                .with_context(|| format!("read semantic manifest {}", manifest_path.display()))?,
        )
        .with_context(|| format!("parse semantic manifest {}", manifest_path.display()))?;
        if manifest.version != CACHE_VERSION
            || manifest.language != language
            || manifest.model != config.model
            || manifest.source_fingerprint != source_fingerprint
            || manifest.embedding_enabled != config.embedding_enabled()
            || manifest.embedding_dimensions != config.embedding_dimensions()
        {
            continue;
        }

        let units_path = cache_dir.join(UNITS_FILE);
        let units = load_units(&units_path)?;

        let units = if manifest.embedding_enabled && manifest.embedding_dimensions > 0 {
            attach_vectors(
                units,
                &cache_dir.join(VECTORS_FILE),
                manifest.embedding_dimensions,
                manifest.unit_count,
            )?
        } else {
            units
        };

        return Ok(Some(SemanticIndex {
            language,
            indexed_files: manifest.indexed_files,
            units,
            embedding_enabled: manifest.embedding_enabled,
            embedding_dimensions: manifest.embedding_dimensions,
            source_fingerprint: manifest.source_fingerprint,
        }));
    }
    Ok(None)
}

pub(crate) fn persist_index(
    project_root: &Path,
    config: &SemanticConfig,
    index: &SemanticIndex,
    source_fingerprint: &str,
) -> Result<()> {
    let cache_dir = writable_cache_dir(project_root, index.language)?;

    persist_units(&cache_dir.join(UNITS_FILE), &index.units)?;
    if index.embedding_enabled && index.embedding_dimensions > 0 {
        persist_vectors(
            &cache_dir.join(VECTORS_FILE),
            &index.units,
            index.embedding_dimensions,
        )?;
    }

    let manifest = SemanticIndexManifest {
        version: CACHE_VERSION,
        language: index.language,
        model: config.model.clone(),
        source_fingerprint: source_fingerprint.to_string(),
        embedding_enabled: index.embedding_enabled,
        embedding_dimensions: index.embedding_dimensions,
        indexed_files: index.indexed_files,
        unit_count: index.units.len(),
        generated_at_unix_secs: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
    };
    fs::write(
        cache_dir.join(MANIFEST_FILE),
        serde_json::to_vec_pretty(&manifest).context("serialize semantic manifest")?,
    )
    .with_context(|| format!("write semantic manifest {}", cache_dir.display()))?;

    Ok(())
}

pub(crate) fn source_fingerprint(
    project_root: &Path,
    files: &[std::path::PathBuf],
) -> Result<String> {
    let mut digest_input = String::new();
    for file in files {
        let relative_path = file
            .strip_prefix(project_root)
            .unwrap_or(file)
            .display()
            .to_string();
        let metadata = fs::metadata(file)
            .with_context(|| format!("read source metadata {}", file.display()))?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_millis())
            .unwrap_or_default();
        digest_input.push_str(&relative_path);
        digest_input.push('|');
        digest_input.push_str(&metadata.len().to_string());
        digest_input.push('|');
        digest_input.push_str(&modified.to_string());
        digest_input.push('\n');
    }
    Ok(format!("{:x}", md5::compute(digest_input)))
}

fn cache_dir(artifact_dir: &Path, language: SupportedLanguage) -> std::path::PathBuf {
    artifact_dir
        .join("cache")
        .join("semantic")
        .join(language.as_str())
}

fn cache_dir_candidates(
    project_root: &Path,
    language: SupportedLanguage,
) -> Vec<std::path::PathBuf> {
    let primary_artifact_dir = daemon_artifact_dir_for_project(project_root);
    let fallback_artifact_dir = temp_artifact_dir_for_project(project_root);
    cache_dir_candidates_for_artifact_dirs(&primary_artifact_dir, &fallback_artifact_dir, language)
}

fn cache_dir_candidates_for_artifact_dirs(
    primary_artifact_dir: &Path,
    fallback_artifact_dir: &Path,
    language: SupportedLanguage,
) -> Vec<std::path::PathBuf> {
    let primary = cache_dir(primary_artifact_dir, language);
    let fallback = cache_dir(fallback_artifact_dir, language);
    if primary == fallback {
        vec![primary]
    } else {
        vec![primary, fallback]
    }
}

fn writable_cache_dir(
    project_root: &Path,
    language: SupportedLanguage,
) -> Result<std::path::PathBuf> {
    let primary_artifact_dir = daemon_artifact_dir_for_project(project_root);
    let fallback_artifact_dir = temp_artifact_dir_for_project(project_root);
    writable_cache_dir_for_artifact_dirs(&primary_artifact_dir, &fallback_artifact_dir, language)
}

fn writable_cache_dir_for_artifact_dirs(
    primary_artifact_dir: &Path,
    fallback_artifact_dir: &Path,
    language: SupportedLanguage,
) -> Result<std::path::PathBuf> {
    let candidates = cache_dir_candidates_for_artifact_dirs(
        primary_artifact_dir,
        fallback_artifact_dir,
        language,
    );
    let mut errors = Vec::new();
    for cache_dir in candidates {
        match fs::create_dir_all(&cache_dir) {
            Ok(()) => return Ok(cache_dir),
            Err(err) => errors.push(format!("{}: {}", cache_dir.display(), err)),
        }
    }
    anyhow::bail!(
        "create semantic cache dir failed for all candidates: {}",
        errors.join("; ")
    );
}

fn load_units(units_path: &Path) -> Result<Vec<EmbeddingUnit>> {
    let file = File::open(units_path)
        .with_context(|| format!("open semantic units {}", units_path.display()))?;
    let reader = BufReader::new(file);
    let mut units = Vec::new();
    for line in reader.lines() {
        let line = line.with_context(|| format!("read semantic units {}", units_path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        let mut unit: EmbeddingUnit = serde_json::from_str(&line)
            .with_context(|| format!("parse semantic unit {}", units_path.display()))?;
        unit.embedding_vector = None;
        units.push(unit);
    }
    Ok(units)
}

fn persist_units(units_path: &Path, units: &[EmbeddingUnit]) -> Result<()> {
    let file = File::create(units_path)
        .with_context(|| format!("create semantic units {}", units_path.display()))?;
    let mut writer = BufWriter::new(file);
    for unit in units {
        let mut persisted = unit.clone();
        persisted.embedding_vector = None;
        serde_json::to_writer(&mut writer, &persisted)
            .with_context(|| format!("serialize semantic unit {}", units_path.display()))?;
        writer
            .write_all(b"\n")
            .with_context(|| format!("write semantic unit {}", units_path.display()))?;
    }
    writer
        .flush()
        .with_context(|| format!("flush semantic units {}", units_path.display()))
}

fn attach_vectors(
    mut units: Vec<EmbeddingUnit>,
    vectors_path: &Path,
    dimensions: usize,
    unit_count: usize,
) -> Result<Vec<EmbeddingUnit>> {
    let bytes = fs::read(vectors_path)
        .with_context(|| format!("read semantic vectors {}", vectors_path.display()))?;
    let expected_bytes = unit_count
        .checked_mul(dimensions)
        .and_then(|value| value.checked_mul(std::mem::size_of::<f32>()))
        .context("semantic vectors byte count overflow")?;
    if bytes.len() != expected_bytes {
        anyhow::bail!(
            "semantic vectors length mismatch: expected {expected_bytes} bytes, got {}",
            bytes.len()
        );
    }

    for (index, unit) in units.iter_mut().enumerate() {
        let start = index * dimensions * std::mem::size_of::<f32>();
        let end = start + dimensions * std::mem::size_of::<f32>();
        let mut vector = Vec::with_capacity(dimensions);
        for chunk in bytes[start..end].chunks_exact(std::mem::size_of::<f32>()) {
            vector.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        unit.embedding_vector = (!vector.iter().all(|value| *value == 0.0)).then_some(vector);
    }

    Ok(units)
}

fn persist_vectors(vectors_path: &Path, units: &[EmbeddingUnit], dimensions: usize) -> Result<()> {
    let file = File::create(vectors_path)
        .with_context(|| format!("create semantic vectors {}", vectors_path.display()))?;
    let mut writer = BufWriter::new(file);
    for unit in units {
        let vector = unit
            .embedding_vector
            .clone()
            .unwrap_or_else(|| vec![0.0; dimensions]);
        if vector.len() != dimensions {
            anyhow::bail!(
                "semantic vector dimension mismatch for {}: expected {dimensions}, got {}",
                unit.path.display(),
                vector.len()
            );
        }
        for value in vector {
            writer
                .write_all(&value.to_le_bytes())
                .with_context(|| format!("write semantic vectors {}", vectors_path.display()))?;
        }
    }
    writer
        .flush()
        .with_context(|| format!("flush semantic vectors {}", vectors_path.display()))
}

#[cfg(test)]
mod tests {
    use super::cache_dir;

    use super::cache_dir_candidates_for_artifact_dirs;
    use super::writable_cache_dir_for_artifact_dirs;
    use crate::daemon::daemon_artifact_dir_for_project;

    use crate::lang_support::SupportedLanguage;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn cache_dir_uses_runtime_artifact_root_instead_of_project_root() {
        let tempdir = tempdir().expect("tempdir should exist");
        let project_root = tempdir.path().join("project");
        std::fs::create_dir_all(&project_root).expect("project root should exist");

        let cache_dir = cache_dir(
            &daemon_artifact_dir_for_project(&project_root),
            SupportedLanguage::Rust,
        );

        assert!(!cache_dir.starts_with(&project_root));
        assert_eq!(
            cache_dir.file_name().and_then(|value| value.to_str()),
            Some("rust")
        );
        assert_eq!(
            cache_dir
                .parent()
                .and_then(|value| value.file_name())
                .and_then(|value| value.to_str()),
            Some("semantic")
        );
        assert_eq!(
            cache_dir
                .parent()
                .and_then(|value| value.parent())
                .and_then(|value| value.file_name())
                .and_then(|value| value.to_str()),
            Some("cache")
        );
    }

    #[test]
    fn cache_dir_candidates_include_temp_fallback_after_runtime_root() {
        let tempdir = tempdir().expect("tempdir should exist");
        let primary_artifact_dir = tempdir.path().join("runtime-artifact");
        let fallback_artifact_dir = tempdir.path().join("temp-artifact");

        let candidates = cache_dir_candidates_for_artifact_dirs(
            &primary_artifact_dir,
            &fallback_artifact_dir,
            SupportedLanguage::Rust,
        );

        assert_eq!(candidates.len(), 2);
        assert_eq!(
            candidates[0],
            cache_dir(&primary_artifact_dir, SupportedLanguage::Rust)
        );
        assert_eq!(
            candidates[1],
            cache_dir(&fallback_artifact_dir, SupportedLanguage::Rust)
        );
    }

    #[cfg(unix)]
    #[test]
    fn writable_cache_dir_falls_back_when_runtime_root_is_blocked() {
        let tempdir = tempdir().expect("tempdir should exist");
        let blocked_primary_artifact_dir = tempdir.path().join("blocked-primary-artifact");
        std::fs::write(&blocked_primary_artifact_dir, "not a directory")
            .expect("blocked primary artifact placeholder should exist");
        let fallback_artifact_dir = tempdir.path().join("fallback-artifact");

        let selected_cache_dir = writable_cache_dir_for_artifact_dirs(
            &blocked_primary_artifact_dir,
            &fallback_artifact_dir,
            SupportedLanguage::Rust,
        )
        .expect("fallback cache dir should be created");

        assert_eq!(
            selected_cache_dir,
            cache_dir(&fallback_artifact_dir, SupportedLanguage::Rust)
        );
    }
}

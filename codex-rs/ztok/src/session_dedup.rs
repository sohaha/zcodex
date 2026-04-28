use crate::compression::CompressionOutputKind;
use crate::compression::CompressionResult;
use crate::compression::ExplicitFallbackReason;
use crate::near_dedup;
use crate::near_dedup::NearDuplicateCandidate;
use crate::near_dedup::NearDuplicateConfig;
use crate::near_dedup::NearDuplicateOutcome;
use crate::session_cache::SessionCacheStore;
use crate::settings;
use anyhow::Result;
use sha1::Digest;
use sha1::Sha1;
use std::path::PathBuf;

pub(crate) fn dedup_read_output(
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
) -> CompressionResult {
    dedup_output(source_name, raw_content, output_signature, result)
}

pub(crate) fn dedup_output(
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
) -> CompressionResult {
    dedup_output_with_runtime_settings(
        source_name,
        raw_content,
        output_signature,
        result,
        &settings::runtime_settings(),
    )
}

#[cfg(test)]
fn dedup_read_output_with_cache_path(
    cache_path: Option<PathBuf>,
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
) -> CompressionResult {
    dedup_read_output_with_cache_path_and_config(
        cache_path,
        source_name,
        raw_content,
        output_signature,
        result,
        NearDuplicateConfig::default(),
    )
}

#[cfg(test)]
fn dedup_read_output_with_cache_path_and_config(
    cache_path: Option<PathBuf>,
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
    near_duplicate_config: NearDuplicateConfig,
) -> CompressionResult {
    dedup_output_with_runtime_settings(
        source_name,
        raw_content,
        output_signature,
        result,
        &settings::ZtokRuntimeSettings::for_test(
            crate::behavior::ZtokBehavior::Enhanced,
            cache_path,
            near_duplicate_config,
        ),
    )
}

fn dedup_output_with_runtime_settings(
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
    runtime_settings: &settings::ZtokRuntimeSettings,
) -> CompressionResult {
    if runtime_settings.behavior.is_basic() {
        return result;
    }

    if runtime_settings.no_cache.enabled {
        return with_additional_fallback(result, ExplicitFallbackReason::DedupDisabledByUser);
    }

    if result.output_kind != CompressionOutputKind::Full {
        return result;
    }

    let Some(cache_path) = runtime_settings.session_cache.db_path.clone() else {
        return with_additional_fallback(result, ExplicitFallbackReason::DedupDisabledNoSessionId);
    };

    match apply_dedup(
        cache_path,
        source_name,
        raw_content,
        output_signature,
        &result.output,
        runtime_settings.near_dedup.text,
    ) {
        Ok(DedupDecision::ExactReference(reference)) => CompressionResult {
            output_kind: CompressionOutputKind::ShortReference,
            output: reference,
            ..result
        },
        Ok(DedupDecision::NearDiff(diff)) => CompressionResult {
            output_kind: CompressionOutputKind::Diff,
            output: diff,
            ..result
        },
        Ok(DedupDecision::Full) => result,
        Ok(DedupDecision::FullFallback(reason)) => with_additional_fallback(result, reason),
        Err(_) => with_additional_fallback(result, ExplicitFallbackReason::DedupCacheUnavailable),
    }
}

fn with_additional_fallback(
    mut result: CompressionResult,
    reason: ExplicitFallbackReason,
) -> CompressionResult {
    if result.fallback.is_none() {
        result.fallback = Some(reason);
    }
    result
}

fn apply_dedup(
    db_path: PathBuf,
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    output: &str,
    near_duplicate_config: NearDuplicateConfig,
) -> Result<DedupDecision> {
    let session_cache = SessionCacheStore::open(&db_path)?;
    let fingerprint = fingerprint(output_signature, output);
    let current_simhash = near_dedup::simhash(raw_content);

    if let Some(previous_source) = session_cache.exact_source_for_fingerprint(&fingerprint)? {
        return Ok(DedupDecision::ExactReference(short_reference(
            &fingerprint,
            &previous_source,
            source_name,
        )));
    }

    let candidates = session_cache.load_near_duplicate_candidates(
        output_signature,
        &fingerprint,
        near_duplicate_config.max_candidate_count,
    )?;
    let candidate_refs: Vec<NearDuplicateCandidate<'_>> = candidates
        .iter()
        .map(|candidate| NearDuplicateCandidate {
            fingerprint: &candidate.fingerprint,
            source_name: &candidate.source_name,
            snapshot: &candidate.snapshot,
            output: &candidate.output,
            simhash: candidate
                .parsed_simhash()
                .unwrap_or_else(|| near_dedup::simhash(&candidate.snapshot)),
            created_at: candidate.created_at,
        })
        .collect();
    let near_duplicate_outcome = near_dedup::analyze_near_duplicate(
        source_name,
        output,
        &fingerprint,
        current_simhash,
        &candidate_refs,
        near_duplicate_config,
    );

    session_cache.store_snapshot(
        &fingerprint,
        source_name,
        output_signature,
        raw_content,
        output,
        current_simhash,
    )?;

    Ok(match near_duplicate_outcome {
        NearDuplicateOutcome::NoMatch => DedupDecision::Full,
        NearDuplicateOutcome::Diff(diff) => DedupDecision::NearDiff(diff),
        NearDuplicateOutcome::Fallback(reason) => DedupDecision::FullFallback(reason),
    })
}

fn fingerprint(output_signature: &str, output: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(output_signature.as_bytes());
    hasher.update([0]);
    hasher.update(output.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn short_reference(fingerprint: &str, previous_source: &str, current_source: &str) -> String {
    let short = &fingerprint[..8];
    if previous_source == current_source {
        format!("[ztok dedup {short}] 同一会话内已输出相同内容，省略重复正文")
    } else {
        format!(
            "[ztok dedup {short}] 同一会话内已输出与 {previous_source} 相同的内容，省略 {current_source} 正文"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::CompressionOutputKind;
    use crate::compression::ContentKind;
    use crate::session_cache::TestSessionCacheRow;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    struct CandidateRow<'a> {
        fingerprint: &'a str,
        source_name: &'a str,
        output_signature: &'a str,
        snapshot: &'a str,
        output: &'a str,
        simhash: &'a str,
        created_at: i64,
    }

    fn insert_candidate_row(cache_path: &Path, candidate: CandidateRow<'_>) {
        let cache = SessionCacheStore::open(cache_path).expect("open cache database");
        cache
            .insert_row_for_test(TestSessionCacheRow {
                fingerprint: candidate.fingerprint,
                source_name: candidate.source_name,
                output_signature: candidate.output_signature,
                snapshot: candidate.snapshot,
                output: candidate.output,
                simhash: candidate.simhash,
                created_at: candidate.created_at,
            })
            .expect("insert candidate row");
    }

    fn full_result(output: &str) -> CompressionResult {
        CompressionResult {
            content_kind: ContentKind::Text,
            output_kind: CompressionOutputKind::Full,
            output: output.to_string(),
            fallback: None,
        }
    }

    fn serialize_simhash(simhash: u64) -> String {
        format!("{simhash:016x}")
    }

    #[test]
    fn missing_session_id_disables_dedup_explicitly() {
        let deduped = dedup_read_output_with_cache_path(
            None,
            "sample.txt",
            "hello",
            "read:none",
            full_result("hello"),
        );
        assert_eq!(deduped.output, "hello");
        assert_eq!(
            deduped.fallback,
            Some(ExplicitFallbackReason::DedupDisabledNoSessionId)
        );
    }

    #[test]
    fn repeated_content_returns_short_reference() {
        let codex_home = TempDir::new().expect("temp dir");
        let cache_path = codex_home
            .path()
            .join(".ztok-cache")
            .join("session-1.sqlite");
        let first = dedup_read_output_with_cache_path(
            Some(cache_path.clone()),
            "alpha.txt",
            "same",
            "read:none",
            full_result("same"),
        );
        assert_eq!(first.output_kind, CompressionOutputKind::Full);

        let second = dedup_read_output_with_cache_path(
            Some(cache_path),
            "alpha.txt",
            "same",
            "read:none",
            full_result("same"),
        );
        assert_eq!(second.output_kind, CompressionOutputKind::ShortReference);
        assert!(second.output.contains("同一会话内已输出相同内容"));
    }

    #[test]
    fn high_similarity_returns_diff_output() {
        let codex_home = TempDir::new().expect("temp dir");
        let cache_path = codex_home
            .path()
            .join(".ztok-cache")
            .join("session-1.sqlite");
        let first = dedup_read_output_with_cache_path(
            Some(cache_path.clone()),
            "alpha.rs",
            "fn main() {\n    let answer = 41;\n    println!(\"{}\", answer);\n}\n",
            "read:none",
            full_result("fn main() {\n    let answer = 41;\n    println!(\"{}\", answer);\n}\n"),
        );
        assert_eq!(first.output_kind, CompressionOutputKind::Full);

        let second = dedup_read_output_with_cache_path(
            Some(cache_path),
            "alpha.rs",
            "fn main() {\n    let answer = 42;\n    println!(\"{}\", answer);\n}\n",
            "read:none",
            full_result("fn main() {\n    let answer = 42;\n    println!(\"{}\", answer);\n}\n"),
        );
        assert_eq!(second.output_kind, CompressionOutputKind::Diff);
        assert!(second.output.contains("let answer = 41;"));
        assert!(second.output.contains("let answer = 42;"));
    }

    #[test]
    fn low_confidence_near_match_falls_back_to_full_output() {
        let codex_home = TempDir::new().expect("temp dir");
        let cache_path = codex_home
            .path()
            .join(".ztok-cache")
            .join("session-1.sqlite");
        let config = NearDuplicateConfig {
            max_hamming_distance: 64,
            min_similarity_ratio: 0.8,
            ..NearDuplicateConfig::default()
        };

        let first = dedup_read_output_with_cache_path_and_config(
            Some(cache_path.clone()),
            "alpha.txt",
            "common a\ncommon b\ncommon c\ncommon d\n",
            "read:none",
            full_result("common a\ncommon b\ncommon c\ncommon d\n"),
            config,
        );
        assert_eq!(first.output_kind, CompressionOutputKind::Full);

        let second = dedup_read_output_with_cache_path_and_config(
            Some(cache_path),
            "alpha.txt",
            "common x\ncommon y\ncommon z\ncommon w\n",
            "read:none",
            full_result("common x\ncommon y\ncommon z\ncommon w\n"),
            config,
        );
        assert_eq!(second.output_kind, CompressionOutputKind::Full);
        assert_eq!(
            second.fallback,
            Some(ExplicitFallbackReason::NearDuplicateLowConfidence)
        );
    }

    #[test]
    fn conflicting_candidates_fall_back_to_full_output() {
        let codex_home = TempDir::new().expect("temp dir");
        let cache_path = codex_home
            .path()
            .join(".ztok-cache")
            .join("session-1.sqlite");
        let config = NearDuplicateConfig {
            conflict_similarity_margin: 0.05,
            ..NearDuplicateConfig::default()
        };
        let output_signature = "read:none";
        let candidate_41_simhash = serialize_simhash(near_dedup::simhash(
            "fn main() {\n    let answer = 41;\n}\n",
        ));
        let candidate_43_simhash = serialize_simhash(near_dedup::simhash(
            "fn main() {\n    let answer = 43;\n}\n",
        ));
        insert_candidate_row(
            &cache_path,
            CandidateRow {
                fingerprint: "candidate-41",
                source_name: "alpha.rs",
                output_signature,
                snapshot: "fn main() {\n    let answer = 41;\n}\n",
                output: "fn main() {\n    let answer = 41;\n}\n",
                simhash: &candidate_41_simhash,
                created_at: 1,
            },
        );
        insert_candidate_row(
            &cache_path,
            CandidateRow {
                fingerprint: "candidate-43",
                source_name: "alpha.rs",
                output_signature,
                snapshot: "fn main() {\n    let answer = 43;\n}\n",
                output: "fn main() {\n    let answer = 43;\n}\n",
                simhash: &candidate_43_simhash,
                created_at: 2,
            },
        );

        let third = dedup_read_output_with_cache_path_and_config(
            Some(cache_path),
            "alpha.rs",
            "fn main() {\n    let answer = 42;\n}\n",
            output_signature,
            full_result("fn main() {\n    let answer = 42;\n}\n"),
            config,
        );
        assert_eq!(third.output_kind, CompressionOutputKind::Full);
        assert_eq!(
            third.fallback,
            Some(ExplicitFallbackReason::NearDuplicateCandidateConflict)
        );
    }

    #[test]
    fn missing_snapshot_falls_back_to_full_output() {
        let codex_home = TempDir::new().expect("temp dir");
        let cache_path = codex_home
            .path()
            .join(".ztok-cache")
            .join("session-1.sqlite");
        let output_signature = "read:none";
        let simhash = serialize_simhash(near_dedup::simhash(
            "fn main() {\n    let answer = 41;\n}\n",
        ));
        insert_candidate_row(
            &cache_path,
            CandidateRow {
                fingerprint: "feedfacecafebeef",
                source_name: "alpha.rs",
                output_signature,
                snapshot: "",
                output: "fn main() {\n    let answer = 41;\n}\n",
                simhash: &simhash,
                created_at: 1,
            },
        );

        let result = dedup_read_output_with_cache_path(
            Some(cache_path),
            "alpha.rs",
            "fn main() {\n    let answer = 42;\n}\n",
            output_signature,
            full_result("fn main() {\n    let answer = 42;\n}\n"),
        );
        assert_eq!(result.output_kind, CompressionOutputKind::Full);
        assert_eq!(
            result.fallback,
            Some(ExplicitFallbackReason::NearDuplicateSnapshotUnavailable)
        );
    }

    #[test]
    fn cache_failures_fall_back_to_full_output() {
        let codex_home = TempDir::new().expect("temp dir");
        let cache_file = codex_home
            .path()
            .join(".ztok-cache")
            .join("session-1.sqlite");
        fs::create_dir_all(&cache_file).expect("create blocking directory");
        let deduped = dedup_read_output_with_cache_path(
            Some(cache_file),
            "alpha.txt",
            "same",
            "read:none",
            full_result("same"),
        );
        assert_eq!(deduped.output_kind, CompressionOutputKind::Full);
        assert_eq!(
            deduped.fallback,
            Some(ExplicitFallbackReason::DedupCacheUnavailable)
        );
    }

    #[test]
    fn corrupted_database_falls_back_to_full_output() {
        let codex_home = TempDir::new().expect("temp dir");
        let cache_file = codex_home
            .path()
            .join(".ztok-cache")
            .join("session-2.sqlite");
        fs::create_dir_all(cache_file.parent().expect("cache dir")).expect("create cache dir");
        fs::write(&cache_file, "not-a-sqlite-database").expect("write corrupted cache file");

        let deduped = dedup_read_output_with_cache_path(
            Some(cache_file),
            "alpha.txt",
            "same",
            "read:none",
            full_result("same"),
        );
        assert_eq!(deduped.output_kind, CompressionOutputKind::Full);
        assert_eq!(
            deduped.fallback,
            Some(ExplicitFallbackReason::DedupCacheUnavailable)
        );
    }
}

#[derive(Debug)]
enum DedupDecision {
    ExactReference(String),
    NearDiff(String),
    Full,
    FullFallback(ExplicitFallbackReason),
}

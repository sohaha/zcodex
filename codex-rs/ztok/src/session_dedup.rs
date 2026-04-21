use crate::behavior::ZtokBehavior;
use crate::compression::CompressionOutputKind;
use crate::compression::CompressionResult;
use crate::compression::ExplicitFallbackReason;
use crate::near_dedup;
use crate::near_dedup::NearDuplicateCandidate;
use crate::near_dedup::NearDuplicateConfig;
use crate::near_dedup::NearDuplicateOutcome;
use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use sha1::Digest;
use sha1::Sha1;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

pub const ZTOK_SESSION_ID_ENV_VAR: &str = "CODEX_ZTOK_SESSION_ID";

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
    dedup_read_output_with_cache_path_and_config(
        session_cache_path(),
        source_name,
        raw_content,
        output_signature,
        result,
        NearDuplicateConfig::default(),
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

fn dedup_read_output_with_cache_path_and_config(
    cache_path: Option<PathBuf>,
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
    near_duplicate_config: NearDuplicateConfig,
) -> CompressionResult {
    if ZtokBehavior::from_env().is_basic() {
        return result;
    }

    if result.output_kind != CompressionOutputKind::Full {
        return result;
    }

    let Some(cache_path) = cache_path else {
        return with_additional_fallback(result, ExplicitFallbackReason::DedupDisabledNoSessionId);
    };

    match apply_dedup(
        cache_path,
        source_name,
        raw_content,
        output_signature,
        &result.output,
        near_duplicate_config,
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

fn session_cache_path() -> Option<PathBuf> {
    let session_id = std::env::var(ZTOK_SESSION_ID_ENV_VAR).ok()?;
    if session_id.trim().is_empty() {
        return None;
    }

    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))?;
    Some(
        codex_home
            .join(".ztok-cache")
            .join(format!("{session_id}.sqlite")),
    )
}

fn apply_dedup(
    db_path: PathBuf,
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    output: &str,
    near_duplicate_config: NearDuplicateConfig,
) -> Result<DedupDecision> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建 ztok 缓存目录失败：{}", parent.display()))?;
    }

    let connection = Connection::open(&db_path)
        .with_context(|| format!("打开 ztok 缓存失败：{}", db_path.display()))?;
    ensure_session_cache_schema(&connection)?;

    let fingerprint = fingerprint(output_signature, output);
    let current_simhash = near_dedup::simhash(raw_content);
    let existing = connection
        .query_row(
            "SELECT source_name FROM session_cache WHERE fingerprint = ?1",
            params![&fingerprint],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(previous_source) = existing {
        return Ok(DedupDecision::ExactReference(short_reference(
            &fingerprint,
            &previous_source,
            source_name,
        )));
    }

    let candidates = load_near_duplicate_candidates(
        &connection,
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
            simhash: parse_simhash(&candidate.simhash_hex)
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

    store_snapshot(
        &connection,
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

fn ensure_session_cache_schema(connection: &Connection) -> Result<()> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_cache (
            fingerprint TEXT PRIMARY KEY,
            source_name TEXT NOT NULL,
            snapshot TEXT NOT NULL,
            output TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );",
    )?;
    ensure_column(connection, "output_signature", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(connection, "simhash", "TEXT NOT NULL DEFAULT ''")?;
    connection.execute(
        "CREATE INDEX IF NOT EXISTS session_cache_output_signature_idx
         ON session_cache(output_signature, created_at DESC)",
        [],
    )?;
    Ok(())
}

fn ensure_column(connection: &Connection, column_name: &str, definition: &str) -> Result<()> {
    let mut statement = connection.prepare("PRAGMA table_info(session_cache)")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        if row.get::<_, String>(1)? == column_name {
            return Ok(());
        }
    }

    connection.execute(
        &format!("ALTER TABLE session_cache ADD COLUMN {column_name} {definition}"),
        [],
    )?;
    Ok(())
}

fn load_near_duplicate_candidates(
    connection: &Connection,
    output_signature: &str,
    current_fingerprint: &str,
    max_candidate_count: usize,
) -> Result<Vec<OwnedCandidateRow>> {
    let mut statement = connection.prepare(
        "SELECT fingerprint, source_name, snapshot, output, simhash, created_at
         FROM session_cache
         WHERE output_signature = ?1 AND fingerprint != ?2
         ORDER BY created_at DESC
         LIMIT ?3",
    )?;
    let rows = statement.query_map(
        params![
            output_signature,
            current_fingerprint,
            max_candidate_count as i64 * 4
        ],
        |row| {
            let fingerprint: String = row.get(0)?;
            let source_name: String = row.get(1)?;
            let snapshot: String = row.get(2)?;
            let output: String = row.get(3)?;
            let simhash_hex: String = row.get(4)?;
            let created_at: i64 = row.get(5)?;
            Ok(OwnedCandidateRow {
                fingerprint,
                source_name,
                snapshot,
                output,
                simhash_hex,
                created_at,
            })
        },
    )?;

    let mut owned_candidates = Vec::new();
    for row in rows {
        owned_candidates.push(row?);
    }

    Ok(owned_candidates)
}

fn store_snapshot(
    connection: &Connection,
    fingerprint: &str,
    source_name: &str,
    output_signature: &str,
    raw_content: &str,
    output: &str,
    simhash: u64,
) -> Result<()> {
    connection.execute(
        "INSERT INTO session_cache (
            fingerprint,
            source_name,
            output_signature,
            snapshot,
            output,
            simhash,
            created_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            fingerprint,
            source_name,
            output_signature,
            raw_content,
            output,
            serialize_simhash(simhash),
            unix_timestamp_now()?,
        ],
    )?;
    Ok(())
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

fn serialize_simhash(simhash: u64) -> String {
    format!("{simhash:016x}")
}

fn parse_simhash(value: &str) -> Option<u64> {
    if value.trim().is_empty() {
        return None;
    }
    u64::from_str_radix(value, 16).ok()
}

fn unix_timestamp_now() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("系统时钟早于 Unix epoch")?;
    Ok(duration.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compression::CompressionOutputKind;
    use crate::compression::ContentKind;
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

    fn insert_candidate_row(cache_path: &PathBuf, candidate: CandidateRow<'_>) {
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).expect("create cache directory");
        }
        let connection = Connection::open(cache_path).expect("open cache database");
        ensure_session_cache_schema(&connection).expect("ensure cache schema");
        connection
            .execute(
                "INSERT INTO session_cache (
                    fingerprint,
                    source_name,
                    output_signature,
                    snapshot,
                    output,
                    simhash,
                    created_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    candidate.fingerprint,
                    candidate.source_name,
                    candidate.output_signature,
                    candidate.snapshot,
                    candidate.output,
                    candidate.simhash,
                    candidate.created_at,
                ],
            )
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
}

#[derive(Debug)]
enum DedupDecision {
    ExactReference(String),
    NearDiff(String),
    Full,
    FullFallback(ExplicitFallbackReason),
}

#[derive(Debug)]
struct OwnedCandidateRow {
    fingerprint: String,
    source_name: String,
    snapshot: String,
    output: String,
    simhash_hex: String,
    created_at: i64,
}

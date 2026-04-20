use crate::compression::CompressionOutputKind;
use crate::compression::CompressionResult;
use crate::compression::ExplicitFallbackReason;
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
    dedup_read_output_with_cache_path(
        session_cache_path(),
        source_name,
        raw_content,
        output_signature,
        result,
    )
}

fn dedup_read_output_with_cache_path(
    cache_path: Option<PathBuf>,
    source_name: &str,
    raw_content: &str,
    output_signature: &str,
    result: CompressionResult,
) -> CompressionResult {
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
    ) {
        Ok(Some(reference)) => CompressionResult {
            output_kind: CompressionOutputKind::ShortReference,
            output: reference,
            ..result
        },
        Ok(None) => result,
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
) -> Result<Option<String>> {
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("创建 ztok 缓存目录失败：{}", parent.display()))?;
    }

    let connection = Connection::open(&db_path)
        .with_context(|| format!("打开 ztok 缓存失败：{}", db_path.display()))?;
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_cache (
            fingerprint TEXT PRIMARY KEY,
            source_name TEXT NOT NULL,
            snapshot TEXT NOT NULL,
            output TEXT NOT NULL,
            created_at INTEGER NOT NULL
        );",
    )?;

    let fingerprint = fingerprint(output_signature, output);
    let existing = connection
        .query_row(
            "SELECT source_name FROM session_cache WHERE fingerprint = ?1",
            params![fingerprint],
            |row| row.get::<_, String>(0),
        )
        .optional()?;

    if let Some(previous_source) = existing {
        return Ok(Some(short_reference(
            &fingerprint,
            &previous_source,
            source_name,
        )));
    }

    connection.execute(
        "INSERT INTO session_cache (fingerprint, source_name, snapshot, output, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            fingerprint,
            source_name,
            raw_content,
            output,
            unix_timestamp_now()?,
        ],
    )?;

    Ok(None)
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

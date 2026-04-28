use crate::session_cache;
use crate::settings;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

const EXPAND_MIN_PREFIX_LEN: usize = 4;
const EXPAND_AMBIGUITY_LIMIT: usize = 6;

pub(crate) fn inspect(session_id: Option<&str>) -> Result<()> {
    let session_id = resolve_session_id(session_id)?;
    let cache_path = session_cache_path(&session_id)?;
    let summary = session_cache::inspect_session_cache(&cache_path)?;

    println!("session: {}", summary.session_id);
    println!("path: {}", summary.path.display());
    println!(
        "status: {}",
        if summary.exists { "present" } else { "absent" }
    );
    println!("entries: {} / {}", summary.entry_count, summary.max_entries);
    println!(
        "schemaVersion: {}",
        summary
            .schema_version
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("sizeBytes: {}", summary.file_size_bytes);
    println!(
        "oldestEntryAt: {}",
        summary
            .oldest_entry_at
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!(
        "newestEntryAt: {}",
        summary
            .newest_entry_at
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    Ok(())
}

pub(crate) fn expand(session_id: Option<&str>, ref_prefix: &str, compressed: bool) -> Result<()> {
    let session_id = resolve_session_id(session_id)?;
    let cache_path = session_cache_path(&session_id)?;
    let prefix = normalize_ref_prefix(ref_prefix)?;
    let cache = session_cache::SessionCacheStore::open(&cache_path)?;
    let rows = cache.expand_rows_by_prefix(&prefix, EXPAND_AMBIGUITY_LIMIT)?;
    let [row] = rows.as_slice() else {
        if rows.is_empty() {
            bail!(
                "未找到匹配的 ztok dedup 引用：session={} prefix={prefix}",
                session_id
            );
        }
        let matches = rows
            .iter()
            .map(|row| format!("{} ({})", row.fingerprint, row.source_name))
            .collect::<Vec<_>>()
            .join(", ");
        bail!("ztok dedup 引用前缀不唯一：prefix={prefix} matches={matches}");
    };

    if compressed {
        print!("{}", row.output);
    } else {
        print!("{}", row.snapshot);
    }
    Ok(())
}

pub(crate) fn clear(session_id: Option<&str>) -> Result<()> {
    let session_id = resolve_session_id(session_id)?;
    let cache_path = session_cache_path(&session_id)?;
    if session_cache::clear_session_cache(&cache_path)? {
        println!(
            "已清空 session cache: {} ({})",
            session_id,
            cache_path.display()
        );
    } else {
        println!(
            "session cache 不存在: {} ({})",
            session_id,
            cache_path.display()
        );
    }
    Ok(())
}

fn resolve_session_id(session_id: Option<&str>) -> Result<String> {
    if let Some(session_id) = sanitize_session_id(session_id) {
        return Ok(session_id.to_string());
    }
    if let Some(session_id) = settings::runtime_settings().session_cache.session_id {
        return Ok(session_id);
    }
    bail!(
        "无法解析当前 ztok session id；请传入 session id，或通过 codex ztok 在有效 CODEX_THREAD_ID 会话中运行"
    );
}

fn session_cache_path(session_id: &str) -> Result<std::path::PathBuf> {
    let Some(session_id) = sanitize_session_id(Some(session_id)) else {
        bail!("无法解析 session cache 路径，session id 不能为空");
    };
    settings::session_cache_path_for_session_id(session_id)
        .with_context(|| format!("无法解析 session cache 路径，session id 不能为空：{session_id}"))
}

fn sanitize_session_id(session_id: Option<&str>) -> Option<&str> {
    let session_id = session_id?.trim();
    if session_id.is_empty() {
        None
    } else {
        Some(session_id)
    }
}

fn normalize_ref_prefix(value: &str) -> Result<String> {
    let trimmed = value.trim();
    let token = if trimmed.starts_with("[ztok dedup ") {
        trimmed
            .trim_start_matches("[ztok dedup ")
            .split_whitespace()
            .next()
            .unwrap_or("")
            .trim_end_matches(']')
    } else {
        trimmed
    };
    let token = token.trim();
    if token.len() < EXPAND_MIN_PREFIX_LEN || !token.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("ztok dedup 引用必须是至少 {EXPAND_MIN_PREFIX_LEN} 位十六进制前缀：{value}");
    }
    Ok(token.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_session_id_is_rejected() {
        let err = session_cache_path("   ").expect_err("empty session id should fail");
        assert!(err.to_string().contains("session id 不能为空"));
    }

    #[test]
    fn explicit_session_id_takes_precedence() {
        assert_eq!(
            resolve_session_id(Some("  thread-explicit  ")).unwrap(),
            "thread-explicit"
        );
    }

    #[test]
    fn normalize_accepts_raw_prefix_and_rendered_dedup_line() {
        assert_eq!(normalize_ref_prefix("ABCDef12").unwrap(), "abcdef12");
        assert_eq!(
            normalize_ref_prefix("[ztok dedup abcdef12] 同一会话内已输出相同内容").unwrap(),
            "abcdef12"
        );
    }

    #[test]
    fn normalize_rejects_short_or_non_hex_prefix() {
        assert!(normalize_ref_prefix("abc").is_err());
        assert!(normalize_ref_prefix("not-a-ref").is_err());
    }
}

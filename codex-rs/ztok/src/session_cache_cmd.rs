use crate::session_cache;
use crate::settings;
use anyhow::Context;
use anyhow::Result;
use anyhow::bail;

pub(crate) fn inspect(session_id: &str) -> Result<()> {
    let cache_path = session_cache_path(session_id)?;
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

pub(crate) fn clear(session_id: &str) -> Result<()> {
    let cache_path = session_cache_path(session_id)?;
    if session_cache::clear_session_cache(&cache_path)? {
        println!(
            "已清空 session cache: {} ({})",
            session_id.trim(),
            cache_path.display()
        );
    } else {
        println!(
            "session cache 不存在: {} ({})",
            session_id.trim(),
            cache_path.display()
        );
    }
    Ok(())
}

fn session_cache_path(session_id: &str) -> Result<std::path::PathBuf> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        bail!("无法解析 session cache 路径，session id 不能为空");
    }
    settings::session_cache_path_for_session_id(session_id).with_context(|| {
        format!(
            "无法解析 session cache 路径，session id 不能为空：{session_id}"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_session_id_is_rejected() {
        let err = session_cache_path("   ").expect_err("empty session id should fail");
        assert!(err.to_string().contains("session id 不能为空"));
    }
}

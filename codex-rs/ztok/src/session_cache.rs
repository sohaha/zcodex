use anyhow::Context;
use anyhow::Result;
use anyhow::bail;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

const SESSION_CACHE_SCHEMA_VERSION: i64 = 1;
const SESSION_CACHE_MAX_ENTRIES: usize = 64;

pub(crate) struct SessionCacheStore {
    connection: Connection,
}

#[derive(Debug)]
pub(crate) struct SessionCacheCandidateRow {
    pub fingerprint: String,
    pub source_name: String,
    pub snapshot: String,
    pub output: String,
    pub simhash_hex: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionCacheSummary {
    pub session_id: String,
    pub path: PathBuf,
    pub exists: bool,
    pub schema_version: Option<i64>,
    pub entry_count: usize,
    pub max_entries: usize,
    pub file_size_bytes: u64,
    pub oldest_entry_at: Option<i64>,
    pub newest_entry_at: Option<i64>,
}

#[cfg(test)]
pub(crate) struct TestSessionCacheRow<'a> {
    pub fingerprint: &'a str,
    pub source_name: &'a str,
    pub output_signature: &'a str,
    pub snapshot: &'a str,
    pub output: &'a str,
    pub simhash: &'a str,
    pub created_at: i64,
}

impl SessionCacheStore {
    pub(crate) fn open(db_path: &Path) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("创建 ztok 缓存目录失败：{}", parent.display()))?;
        }

        let connection = Connection::open(db_path)
            .with_context(|| format!("打开 ztok 缓存失败：{}", db_path.display()))?;
        ensure_session_cache_schema(&connection)?;
        prune_excess_rows(&connection, SESSION_CACHE_MAX_ENTRIES)?;
        Ok(Self { connection })
    }

    pub(crate) fn exact_source_for_fingerprint(&self, fingerprint: &str) -> Result<Option<String>> {
        self.connection
            .query_row(
                "SELECT source_name FROM session_cache WHERE fingerprint = ?1",
                params![fingerprint],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub(crate) fn load_near_duplicate_candidates(
        &self,
        output_signature: &str,
        current_fingerprint: &str,
        max_candidate_count: usize,
    ) -> Result<Vec<SessionCacheCandidateRow>> {
        let mut statement = self.connection.prepare(
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
                Ok(SessionCacheCandidateRow {
                    fingerprint: row.get(0)?,
                    source_name: row.get(1)?,
                    snapshot: row.get(2)?,
                    output: row.get(3)?,
                    simhash_hex: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )?;

        let mut candidates = Vec::new();
        for row in rows {
            candidates.push(row?);
        }
        Ok(candidates)
    }

    pub(crate) fn store_snapshot(
        &self,
        fingerprint: &str,
        source_name: &str,
        output_signature: &str,
        raw_content: &str,
        output: &str,
        simhash: u64,
    ) -> Result<()> {
        self.connection.execute(
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
        prune_excess_rows(&self.connection, SESSION_CACHE_MAX_ENTRIES)?;
        Ok(())
    }

    pub(crate) fn summary(&self, db_path: &Path) -> Result<SessionCacheSummary> {
        let entry_count =
            self.connection
                .query_row("SELECT COUNT(*) FROM session_cache", [], |row| {
                    row.get::<_, i64>(0)
                })?;
        let oldest_entry_at =
            self.connection
                .query_row("SELECT MIN(created_at) FROM session_cache", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })?;
        let newest_entry_at =
            self.connection
                .query_row("SELECT MAX(created_at) FROM session_cache", [], |row| {
                    row.get::<_, Option<i64>>(0)
                })?;
        Ok(SessionCacheSummary {
            session_id: session_id_from_path(db_path),
            path: db_path.to_path_buf(),
            exists: db_path.exists(),
            schema_version: read_metadata_i64(&self.connection, "schema_version")?,
            entry_count: entry_count as usize,
            max_entries: SESSION_CACHE_MAX_ENTRIES,
            file_size_bytes: fs::metadata(db_path)
                .map(|metadata| metadata.len())
                .unwrap_or(0),
            oldest_entry_at,
            newest_entry_at,
        })
    }

    #[cfg(test)]
    pub(crate) fn insert_row_for_test(&self, row: TestSessionCacheRow<'_>) -> Result<()> {
        self.connection.execute(
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
                row.fingerprint,
                row.source_name,
                row.output_signature,
                row.snapshot,
                row.output,
                row.simhash,
                row.created_at,
            ],
        )?;
        Ok(())
    }
}

impl SessionCacheCandidateRow {
    pub(crate) fn parsed_simhash(&self) -> Option<u64> {
        if self.simhash_hex.trim().is_empty() {
            return None;
        }
        u64::from_str_radix(&self.simhash_hex, 16).ok()
    }
}

pub(crate) fn inspect_session_cache(db_path: &Path) -> Result<SessionCacheSummary> {
    if !db_path.exists() {
        return Ok(SessionCacheSummary {
            session_id: session_id_from_path(db_path),
            path: db_path.to_path_buf(),
            exists: false,
            schema_version: None,
            entry_count: 0,
            max_entries: SESSION_CACHE_MAX_ENTRIES,
            file_size_bytes: 0,
            oldest_entry_at: None,
            newest_entry_at: None,
        });
    }

    SessionCacheStore::open(db_path)?.summary(db_path)
}

pub(crate) fn clear_session_cache(db_path: &Path) -> Result<bool> {
    if !db_path.exists() {
        return Ok(false);
    }

    if db_path.is_dir() {
        fs::remove_dir_all(db_path)
            .with_context(|| format!("删除损坏的 session cache 目录失败：{}", db_path.display()))?;
    } else {
        fs::remove_file(db_path)
            .with_context(|| format!("删除 session cache 文件失败：{}", db_path.display()))?;
    }
    Ok(true)
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
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_cache_metadata (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );",
    )?;
    ensure_column(connection, "output_signature", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(connection, "simhash", "TEXT NOT NULL DEFAULT ''")?;
    ensure_metadata_value(
        connection,
        "schema_version",
        &SESSION_CACHE_SCHEMA_VERSION.to_string(),
    )?;
    ensure_metadata_value(
        connection,
        "max_entries",
        &SESSION_CACHE_MAX_ENTRIES.to_string(),
    )?;
    let schema_version =
        read_metadata_i64(connection, "schema_version")?.unwrap_or(SESSION_CACHE_SCHEMA_VERSION);
    if schema_version != SESSION_CACHE_SCHEMA_VERSION {
        bail!(
            "unsupported ztok session cache schema version: expected {SESSION_CACHE_SCHEMA_VERSION}, got {schema_version}"
        );
    }
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

fn ensure_metadata_value(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection.execute(
        "INSERT OR IGNORE INTO session_cache_metadata (key, value) VALUES (?1, ?2)",
        params![key, value],
    )?;
    Ok(())
}

fn read_metadata_i64(connection: &Connection, key: &str) -> Result<Option<i64>> {
    connection
        .query_row(
            "SELECT value FROM session_cache_metadata WHERE key = ?1",
            params![key],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .map(|value| {
            value
                .parse::<i64>()
                .with_context(|| format!("解析 session cache metadata 失败：{key}={value}"))
        })
        .transpose()
}

fn prune_excess_rows(connection: &Connection, max_entries: usize) -> Result<usize> {
    let entry_count = connection.query_row("SELECT COUNT(*) FROM session_cache", [], |row| {
        row.get::<_, i64>(0)
    })?;
    let overflow = entry_count - max_entries as i64;
    if overflow <= 0 {
        return Ok(0);
    }

    let deleted = connection.execute(
        "DELETE FROM session_cache
         WHERE fingerprint IN (
             SELECT fingerprint
             FROM session_cache
             ORDER BY created_at ASC, fingerprint ASC
             LIMIT ?1
         )",
        params![overflow],
    )?;
    Ok(deleted)
}

fn serialize_simhash(simhash: u64) -> String {
    format!("{simhash:016x}")
}

fn unix_timestamp_now() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("系统时钟早于 Unix epoch")?;
    Ok(duration.as_secs() as i64)
}

fn session_id_from_path(db_path: &Path) -> String {
    db_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn inspect_missing_cache_reports_absent() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join(".ztok-cache").join("missing.sqlite");
        let summary = inspect_session_cache(&path).expect("inspect missing cache");

        assert_eq!(
            summary,
            SessionCacheSummary {
                session_id: "missing".to_string(),
                path,
                exists: false,
                schema_version: None,
                entry_count: 0,
                max_entries: SESSION_CACHE_MAX_ENTRIES,
                file_size_bytes: 0,
                oldest_entry_at: None,
                newest_entry_at: None,
            }
        );
    }

    #[test]
    fn inspect_reports_entries_and_schema_version() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join(".ztok-cache").join("thread-1.sqlite");
        let cache = SessionCacheStore::open(&path).expect("open cache");
        cache
            .insert_row_for_test(TestSessionCacheRow {
                fingerprint: "f1",
                source_name: "alpha.txt",
                output_signature: "read:none",
                snapshot: "alpha",
                output: "alpha",
                simhash: "00",
                created_at: 42,
            })
            .expect("insert row");

        let summary = inspect_session_cache(&path).expect("inspect cache");
        assert!(summary.exists);
        assert_eq!(summary.schema_version, Some(SESSION_CACHE_SCHEMA_VERSION));
        assert_eq!(summary.entry_count, 1);
        assert_eq!(summary.oldest_entry_at, Some(42));
        assert_eq!(summary.newest_entry_at, Some(42));
    }

    #[test]
    fn open_rejects_unsupported_schema_version() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join(".ztok-cache").join("thread-2.sqlite");
        let cache = SessionCacheStore::open(&path).expect("open cache");
        cache
            .connection
            .execute(
                "UPDATE session_cache_metadata SET value = '999' WHERE key = 'schema_version'",
                [],
            )
            .expect("update metadata");

        let err = match SessionCacheStore::open(&path) {
            Ok(_) => panic!("unsupported schema should fail"),
            Err(err) => err,
        };
        assert!(
            err.to_string()
                .contains("unsupported ztok session cache schema version")
        );
    }

    #[test]
    fn store_snapshot_prunes_old_rows() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join(".ztok-cache").join("thread-3.sqlite");
        let cache = SessionCacheStore::open(&path).expect("open cache");
        for index in 0..(SESSION_CACHE_MAX_ENTRIES + 3) {
            cache
                .insert_row_for_test(TestSessionCacheRow {
                    fingerprint: Box::leak(format!("fingerprint-{index}").into_boxed_str()),
                    source_name: "alpha.txt",
                    output_signature: "read:none",
                    snapshot: "alpha",
                    output: "alpha",
                    simhash: "00",
                    created_at: index as i64,
                })
                .expect("insert row");
        }
        prune_excess_rows(&cache.connection, SESSION_CACHE_MAX_ENTRIES).expect("prune rows");

        let summary = inspect_session_cache(&path).expect("inspect cache");
        assert_eq!(summary.entry_count, SESSION_CACHE_MAX_ENTRIES);
        assert_eq!(summary.oldest_entry_at, Some(3));
    }

    #[test]
    fn clear_removes_directory_backed_cache() {
        let temp = TempDir::new().expect("temp dir");
        let path = temp.path().join(".ztok-cache").join("thread-4.sqlite");
        fs::create_dir_all(&path).expect("create cache dir");

        assert!(clear_session_cache(&path).expect("clear cache"));
        assert!(!path.exists());
    }
}

use anyhow::Context;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use rusqlite::params;
use std::fs;
use std::path::Path;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

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
        Ok(())
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

fn serialize_simhash(simhash: u64) -> String {
    format!("{simhash:016x}")
}

fn unix_timestamp_now() -> Result<i64> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("系统时钟早于 Unix epoch")?;
    Ok(duration.as_secs() as i64)
}

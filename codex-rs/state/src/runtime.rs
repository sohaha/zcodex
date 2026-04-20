use crate::AgentJob;
use crate::AgentJobCreateParams;
use crate::AgentJobItem;
use crate::AgentJobItemCreateParams;
use crate::AgentJobItemStatus;
use crate::AgentJobProgress;
use crate::AgentJobStatus;
use crate::LOGS_DB_FILENAME;
use crate::LOGS_DB_VERSION;
use crate::LogEntry;
use crate::LogQuery;
use crate::LogRow;
use crate::STATE_DB_FILENAME;
use crate::STATE_DB_VERSION;
use crate::SortKey;
use crate::ThreadMetadata;
use crate::ThreadMetadataBuilder;
use crate::ThreadsPage;
use crate::apply_rollout_item;
use crate::migrations::STATE_MIGRATOR;
use crate::migrations::runtime_logs_migrator;
use crate::migrations::runtime_state_migrator;
use crate::model::AgentJobRow;
use crate::model::ThreadRow;
use crate::model::anchor_from_item;
use crate::model::datetime_to_epoch_millis;
use crate::model::datetime_to_epoch_seconds;
use crate::model::epoch_millis_to_datetime;
use crate::paths::file_modified_time_utc;
use chrono::DateTime;
use chrono::Utc;
use codex_protocol::ThreadId;
use codex_protocol::dynamic_tools::DynamicToolSpec;
use codex_protocol::protocol::RolloutItem;
use log::LevelFilter;
use serde_json::Value;
use sqlx::ConnectOptions;
use sqlx::QueryBuilder;
use sqlx::Row;
use sqlx::Sqlite;
use sqlx::SqliteConnection;
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;
use sqlx::sqlite::SqliteAutoVacuum;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqliteJournalMode;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::sqlite::SqliteSynchronous;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::Duration;
use tracing::warn;

mod agent_jobs;
mod backfill;
mod logs;
mod memories;
mod remote_control;
#[cfg(test)]
mod test_support;
mod threads;

pub use remote_control::RemoteControlEnrollmentRecord;
pub use threads::ThreadFilterOptions;

// "Partition" is the retained-log-content bucket we cap at 10 MiB:
// - one bucket per non-null thread_id
// - one bucket per threadless (thread_id IS NULL) non-null process_uuid
// - one bucket for threadless rows with process_uuid IS NULL
// This budget tracks each row's persisted rendered log body plus non-body
// metadata, rather than the exact sum of all persisted SQLite column bytes.
const LOG_PARTITION_SIZE_LIMIT_BYTES: i64 = 10 * 1024 * 1024;
const LOG_PARTITION_ROW_LIMIT: i64 = 1_000;
const LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION: i64 = 9_001;

#[derive(Clone)]
pub struct StateRuntime {
    codex_home: PathBuf,
    default_provider: String,
    pool: Arc<sqlx::SqlitePool>,
    logs_pool: Arc<sqlx::SqlitePool>,
    thread_updated_at_millis: Arc<AtomicI64>,
}

impl StateRuntime {
    /// Initialize the state runtime using the provided Codex home and default provider.
    ///
    /// This opens (and migrates) the SQLite databases under `codex_home`,
    /// keeping logs in a dedicated file to reduce lock contention with the
    /// rest of the state store.
    pub async fn init(codex_home: PathBuf, default_provider: String) -> anyhow::Result<Arc<Self>> {
        tokio::fs::create_dir_all(&codex_home).await?;
        let state_migrator = runtime_state_migrator();
        let logs_migrator = runtime_logs_migrator();
        let current_state_name = state_db_filename();
        let current_logs_name = logs_db_filename();
        remove_legacy_db_files(
            &codex_home,
            current_state_name.as_str(),
            STATE_DB_FILENAME,
            "state",
        )
        .await;
        remove_legacy_db_files(
            &codex_home,
            current_logs_name.as_str(),
            LOGS_DB_FILENAME,
            "logs",
        )
        .await;
        let state_path = state_db_path(codex_home.as_path());
        let logs_path = logs_db_path(codex_home.as_path());
        let pool = match open_state_sqlite(&state_path, &state_migrator).await {
            Ok(db) => Arc::new(db),
            Err(err) => {
                warn!("failed to open state db at {}: {err}", state_path.display());
                return Err(err);
            }
        };
        let logs_pool = match open_logs_sqlite(&logs_path, &logs_migrator).await {
            Ok(db) => Arc::new(db),
            Err(err) => {
                warn!("failed to open logs db at {}: {err}", logs_path.display());
                return Err(err);
            }
        };
        let thread_updated_at_millis: Option<i64> =
            sqlx::query_scalar("SELECT MAX(threads.updated_at_ms) FROM threads")
                .fetch_one(pool.as_ref())
                .await?;
        let thread_updated_at_millis = thread_updated_at_millis.unwrap_or(0);
        let runtime = Arc::new(Self {
            pool,
            logs_pool,
            codex_home,
            default_provider,
            thread_updated_at_millis: Arc::new(AtomicI64::new(thread_updated_at_millis)),
        });
        if let Err(err) = runtime.run_logs_startup_maintenance().await {
            warn!(
                "failed to run startup maintenance for logs db at {}: {err}",
                logs_path.display(),
            );
        }
        Ok(runtime)
    }

    /// Return the configured Codex home directory for this runtime.
    pub fn codex_home(&self) -> &Path {
        self.codex_home.as_path()
    }
}

fn base_sqlite_options(path: &Path) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(5))
        .log_statements(LevelFilter::Off)
}

async fn open_state_sqlite(path: &Path, migrator: &Migrator) -> anyhow::Result<SqlitePool> {
    let options = base_sqlite_options(path).auto_vacuum(SqliteAutoVacuum::Incremental);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    repair_legacy_local_state_migration_history(path, &pool).await?;
    migrator.run(&pool).await?;
    let auto_vacuum = sqlx::query_scalar::<_, i64>("PRAGMA auto_vacuum")
        .fetch_one(&pool)
        .await?;
    if auto_vacuum != SqliteAutoVacuum::Incremental as i64 {
        // Existing state DBs need one non-transactional `VACUUM` before
        // SQLite persists `auto_vacuum = INCREMENTAL` in the database header.
        sqlx::query("PRAGMA auto_vacuum = INCREMENTAL")
            .execute(&pool)
            .await?;
        // We do it on best effort. If the lock can't be acquired, it will be done at next run.
        let _ = sqlx::query("VACUUM").execute(&pool).await;
    }
    // We do it on best effort. If the lock can't be acquired, it will be done at next run.
    let _ = sqlx::query("PRAGMA incremental_vacuum")
        .execute(&pool)
        .await;
    Ok(pool)
}

async fn repair_legacy_local_state_migration_history(
    path: &Path,
    pool: &SqlitePool,
) -> anyhow::Result<()> {
    if !sqlite_table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }

    let applied_migrations = applied_state_migrations_by_version(pool).await?;
    let mut planned_moves = Vec::new();
    for (source_version, target_version, expected_migration) in [
        (
            23_i64,
            LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION,
            state_migration_by_version(LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION),
        ),
        (24_i64, 23_i64, state_migration_by_version(23)),
        (25_i64, 24_i64, state_migration_by_version(24)),
    ] {
        let Some(applied_migration) = applied_migrations.get(&source_version) else {
            continue;
        };
        if applied_migration.description == expected_migration.description
            && applied_migration.checksum == expected_migration.checksum.as_ref()
        {
            planned_moves.push((source_version, target_version));
        }
    }
    if planned_moves.is_empty() {
        return Ok(());
    }

    let planned_sources: BTreeSet<i64> = planned_moves
        .iter()
        .map(|(source_version, _)| *source_version)
        .collect();
    for (_, target_version) in &planned_moves {
        if applied_migrations.contains_key(target_version)
            && !planned_sources.contains(target_version)
        {
            warn!(
                "refusing to repair ambiguous legacy state migration history at {}: target version {} is already occupied",
                path.display(),
                target_version,
            );
            return Ok(());
        }
    }

    warn!(
        "repairing legacy state migration history drift at {}",
        path.display()
    );
    let mut tx = pool.begin().await?;
    for (index, (source_version, _)) in planned_moves.iter().enumerate() {
        let temp_version = -1 - i64::try_from(index).expect("small staged migration count");
        sqlx::query("UPDATE _sqlx_migrations SET version = ? WHERE version = ?")
            .bind(temp_version)
            .bind(*source_version)
            .execute(&mut *tx)
            .await?;
    }
    for (index, (_, target_version)) in planned_moves.iter().enumerate() {
        let temp_version = -1 - i64::try_from(index).expect("small staged migration count");
        sqlx::query("UPDATE _sqlx_migrations SET version = ? WHERE version = ?")
            .bind(*target_version)
            .bind(temp_version)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

struct AppliedStateMigration {
    description: String,
    checksum: Vec<u8>,
}

async fn applied_state_migrations_by_version(
    pool: &SqlitePool,
) -> anyhow::Result<HashMap<i64, AppliedStateMigration>> {
    let rows = sqlx::query(
        "SELECT version, description, checksum FROM _sqlx_migrations WHERE success = 1",
    )
    .fetch_all(pool)
    .await?;

    let mut applied_migrations = HashMap::with_capacity(rows.len());
    for row in rows {
        let version: i64 = row.try_get("version")?;
        let description: String = row.try_get("description")?;
        let checksum: Vec<u8> = row.try_get("checksum")?;
        applied_migrations.insert(
            version,
            AppliedStateMigration {
                description,
                checksum,
            },
        );
    }
    Ok(applied_migrations)
}

fn state_migration_by_version(version: i64) -> &'static sqlx::migrate::Migration {
    STATE_MIGRATOR
        .migrations
        .iter()
        .find(|migration| migration.version == version)
        .expect("state migration should exist")
}

async fn sqlite_table_exists(pool: &SqlitePool, table: &str) -> anyhow::Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

async fn sqlite_column_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
) -> anyhow::Result<bool> {
    if !sqlite_table_exists(pool, table).await? {
        return Ok(false);
    }

    let pragma = format!("PRAGMA table_info(\"{table}\")");
    let rows = sqlx::query(&pragma).fetch_all(pool).await?;
    for row in rows {
        let name: String = row.try_get("name")?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn open_logs_sqlite(path: &Path, migrator: &Migrator) -> anyhow::Result<SqlitePool> {
    let options = base_sqlite_options(path).auto_vacuum(SqliteAutoVacuum::Incremental);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    migrator.run(&pool).await?;
    Ok(pool)
}

fn db_filename(base_name: &str, version: u32) -> String {
    format!("{base_name}_{version}.sqlite")
}

pub fn state_db_filename() -> String {
    db_filename(STATE_DB_FILENAME, STATE_DB_VERSION)
}

pub fn state_db_path(codex_home: &Path) -> PathBuf {
    codex_home.join(state_db_filename())
}

pub fn logs_db_filename() -> String {
    db_filename(LOGS_DB_FILENAME, LOGS_DB_VERSION)
}

pub fn logs_db_path(codex_home: &Path) -> PathBuf {
    codex_home.join(logs_db_filename())
}

async fn remove_legacy_db_files(
    codex_home: &Path,
    current_name: &str,
    base_name: &str,
    db_label: &str,
) {
    let mut entries = match tokio::fs::read_dir(codex_home).await {
        Ok(entries) => entries,
        Err(err) => {
            warn!(
                "failed to read codex_home for {db_label} db cleanup {}: {err}",
                codex_home.display(),
            );
            return;
        }
    };
    let mut legacy_paths = Vec::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if !entry
            .file_type()
            .await
            .map(|file_type| file_type.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if !should_remove_db_file(file_name.as_ref(), current_name, base_name) {
            continue;
        }

        legacy_paths.push(entry.path());
    }

    // On Windows, SQLite can keep the main database file undeletable until the
    // matching `-wal` / `-shm` sidecars are removed. Remove the longest
    // sidecar-style paths first so the main file is attempted last.
    legacy_paths.sort_by_key(|path| std::cmp::Reverse(path.as_os_str().len()));
    for legacy_path in legacy_paths {
        if let Err(err) = tokio::fs::remove_file(&legacy_path).await {
            warn!(
                "failed to remove legacy {db_label} db file {}: {err}",
                legacy_path.display(),
            );
        }
    }
}

fn should_remove_db_file(file_name: &str, current_name: &str, base_name: &str) -> bool {
    let mut normalized_name = file_name;
    for suffix in ["-wal", "-shm", "-journal"] {
        if let Some(stripped) = file_name.strip_suffix(suffix) {
            normalized_name = stripped;
            break;
        }
    }
    if normalized_name == current_name {
        return false;
    }
    let unversioned_name = format!("{base_name}.sqlite");
    if normalized_name == unversioned_name {
        return true;
    }

    let Some(version_with_extension) = normalized_name.strip_prefix(&format!("{base_name}_"))
    else {
        return false;
    };
    let Some(version_suffix) = version_with_extension.strip_suffix(".sqlite") else {
        return false;
    };
    !version_suffix.is_empty() && version_suffix.chars().all(|ch| ch.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION;
    use super::applied_state_migrations_by_version;
    use super::open_state_sqlite;
    use super::runtime_state_migrator;
    use super::sqlite_column_exists;
    use super::sqlite_table_exists;
    use super::state_db_path;
    use super::state_migration_by_version;
    use super::test_support::unique_temp_dir;
    use crate::migrations::STATE_MIGRATOR;
    use sqlx::SqlitePool;
    use sqlx::migrate::MigrateError;
    use sqlx::migrate::Migrator;
    use sqlx::sqlite::SqliteConnectOptions;
    use std::borrow::Cow;
    use std::collections::BTreeSet;
    use std::path::Path;

    async fn open_db_pool(path: &Path) -> SqlitePool {
        SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false),
        )
        .await
        .expect("open sqlite pool")
    }

    #[tokio::test]
    async fn open_state_sqlite_tolerates_newer_applied_migrations() {
        let codex_home = unique_temp_dir();
        tokio::fs::create_dir_all(&codex_home)
            .await
            .expect("create codex home");
        let state_path = state_db_path(codex_home.as_path());
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&state_path)
                .create_if_missing(true),
        )
        .await
        .expect("open state db");
        STATE_MIGRATOR
            .run(&pool)
            .await
            .expect("apply current state schema");
        sqlx::query(
            "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(9_999_i64)
        .bind("future migration")
        .bind(true)
        .bind(vec![1_u8, 2, 3, 4])
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("insert future migration record");
        pool.close().await;

        let strict_pool = open_db_pool(state_path.as_path()).await;
        let strict_err = STATE_MIGRATOR
            .run(&strict_pool)
            .await
            .expect_err("strict migrator should reject newer applied migrations");
        assert!(matches!(strict_err, MigrateError::VersionMissing(9_999)));
        strict_pool.close().await;

        let tolerant_migrator = runtime_state_migrator();
        let tolerant_pool = open_state_sqlite(state_path.as_path(), &tolerant_migrator)
            .await
            .expect("runtime migrator should tolerate newer applied migrations");
        tolerant_pool.close().await;

        let _ = tokio::fs::remove_dir_all(codex_home).await;
    }

    fn legacy_local_state_migrator(
        applied_legacy_versions: &[i64],
        applied_current_versions: &[i64],
    ) -> Migrator {
        let applied_legacy_versions: BTreeSet<i64> =
            applied_legacy_versions.iter().copied().collect();
        let applied_current_versions: BTreeSet<i64> =
            applied_current_versions.iter().copied().collect();
        let mut selected_versions: BTreeSet<i64> = (1_i64..=22_i64).collect();
        selected_versions.extend(applied_legacy_versions.iter().copied());
        selected_versions.extend(applied_current_versions.iter().copied());
        let mut legacy_max_retries =
            state_migration_by_version(LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION)
                .clone();
        legacy_max_retries.version = 23;
        let mut legacy_drop_logs = state_migration_by_version(23).clone();
        legacy_drop_logs.version = 24;
        let mut legacy_remote_control = state_migration_by_version(24).clone();
        legacy_remote_control.version = 25;

        let legacy_migrations = [legacy_max_retries, legacy_drop_logs, legacy_remote_control];
        Migrator {
            migrations: Cow::Owned(
                STATE_MIGRATOR
                    .migrations
                    .iter()
                    .filter(|migration| migration.version <= 22)
                    .cloned()
                    .chain(
                        legacy_migrations.into_iter().filter(|migration| {
                            applied_legacy_versions.contains(&migration.version)
                        }),
                    )
                    .chain(
                        STATE_MIGRATOR
                            .migrations
                            .iter()
                            .filter(|migration| {
                                applied_current_versions.contains(&migration.version)
                            })
                            .cloned(),
                    )
                    .collect(),
            ),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        }
    }

    #[tokio::test]
    async fn open_state_sqlite_repairs_legacy_local_migration_numbering_drift() {
        let codex_home = unique_temp_dir();
        tokio::fs::create_dir_all(&codex_home)
            .await
            .expect("create codex home");
        let state_path = state_db_path(codex_home.as_path());
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&state_path)
                .create_if_missing(true),
        )
        .await
        .expect("open legacy state db");
        legacy_local_state_migrator(&[23, 24, 25], &[])
            .run(&pool)
            .await
            .expect("apply legacy local state schema");
        pool.close().await;

        let repaired_pool = open_state_sqlite(state_path.as_path(), &runtime_state_migrator())
            .await
            .expect("runtime should repair legacy local migration numbering drift");

        assert!(
            sqlite_column_exists(&repaired_pool, "agent_jobs", "max_retries")
                .await
                .expect("check max_retries column")
        );
        assert!(
            sqlite_table_exists(&repaired_pool, "remote_control_enrollments")
                .await
                .expect("check remote_control_enrollments table")
        );
        assert!(
            sqlite_column_exists(&repaired_pool, "threads", "created_at_ms")
                .await
                .expect("check created_at_ms column")
        );
        assert!(
            sqlite_column_exists(&repaired_pool, "threads", "updated_at_ms")
                .await
                .expect("check updated_at_ms column")
        );

        let applied_versions: Vec<i64> =
            sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
                .fetch_all(&repaired_pool)
                .await
                .expect("read applied migration versions");
        assert!(applied_versions.contains(&23));
        assert!(applied_versions.contains(&24));
        assert!(applied_versions.contains(&25));
        assert!(applied_versions.contains(&LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION));

        repaired_pool.close().await;

        let _ = tokio::fs::remove_dir_all(codex_home).await;
    }

    #[tokio::test]
    async fn open_state_sqlite_repairs_partial_legacy_local_migration_history() {
        let codex_home = unique_temp_dir();
        tokio::fs::create_dir_all(&codex_home)
            .await
            .expect("create codex home");
        let state_path = state_db_path(codex_home.as_path());
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&state_path)
                .create_if_missing(true),
        )
        .await
        .expect("open legacy state db");
        legacy_local_state_migrator(&[23], &[])
            .run(&pool)
            .await
            .expect("apply partial legacy local state schema");
        pool.close().await;

        let repaired_pool = open_state_sqlite(state_path.as_path(), &runtime_state_migrator())
            .await
            .expect("runtime should repair partial legacy local migration history");

        let applied_migrations = applied_state_migrations_by_version(&repaired_pool)
            .await
            .expect("read applied migration versions");
        assert!(applied_migrations.contains_key(&23));
        assert!(applied_migrations.contains_key(&24));
        assert!(applied_migrations.contains_key(&25));
        assert!(
            applied_migrations.contains_key(&LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION)
        );

        repaired_pool.close().await;

        let _ = tokio::fs::remove_dir_all(codex_home).await;
    }

    #[tokio::test]
    async fn open_state_sqlite_does_not_repair_unknown_migration_checksums() {
        let codex_home = unique_temp_dir();
        tokio::fs::create_dir_all(&codex_home)
            .await
            .expect("create codex home");
        let state_path = state_db_path(codex_home.as_path());
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&state_path)
                .create_if_missing(true),
        )
        .await
        .expect("open legacy state db");
        legacy_local_state_migrator(&[23], &[])
            .run(&pool)
            .await
            .expect("apply partial legacy local state schema");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ? WHERE version = 23")
            .bind(vec![0_u8, 1, 2, 3])
            .execute(&pool)
            .await
            .expect("tamper legacy checksum");
        pool.close().await;

        let err = open_state_sqlite(state_path.as_path(), &runtime_state_migrator())
            .await
            .expect_err("runtime should not repair unknown migration checksums");
        let err_text = format!("{err:#}");
        assert!(err_text.contains("migration 23"));

        let _ = tokio::fs::remove_dir_all(codex_home).await;
    }

    #[tokio::test]
    async fn open_state_sqlite_repairs_hybrid_legacy_and_current_migration_history() {
        let codex_home = unique_temp_dir();
        tokio::fs::create_dir_all(&codex_home)
            .await
            .expect("create codex home");
        let state_path = state_db_path(codex_home.as_path());
        let pool = SqlitePool::connect_with(
            SqliteConnectOptions::new()
                .filename(&state_path)
                .create_if_missing(true),
        )
        .await
        .expect("open hybrid state db");
        legacy_local_state_migrator(&[23, 24], &[25])
            .run(&pool)
            .await
            .expect("apply hybrid legacy/current state schema");
        pool.close().await;

        let repaired_pool = open_state_sqlite(state_path.as_path(), &runtime_state_migrator())
            .await
            .expect("runtime should repair hybrid legacy/current migration history");

        let applied_migrations = applied_state_migrations_by_version(&repaired_pool)
            .await
            .expect("read applied migration versions");
        assert!(applied_migrations.contains_key(&23));
        assert!(applied_migrations.contains_key(&24));
        assert!(applied_migrations.contains_key(&25));
        assert!(
            applied_migrations.contains_key(&LOCAL_STATE_AGENT_JOBS_MAX_RETRIES_MIGRATION_VERSION)
        );
        assert!(
            sqlite_table_exists(&repaired_pool, "remote_control_enrollments")
                .await
                .expect("check remote_control_enrollments table")
        );
        assert!(
            sqlite_column_exists(&repaired_pool, "threads", "created_at_ms")
                .await
                .expect("check created_at_ms column")
        );
        assert!(
            sqlite_column_exists(&repaired_pool, "threads", "updated_at_ms")
                .await
                .expect("check updated_at_ms column")
        );

        repaired_pool.close().await;

        let _ = tokio::fs::remove_dir_all(codex_home).await;
    }
}

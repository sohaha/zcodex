use crate::config::ZmemoryConfig;
use crate::schema::initialize_database;
use anyhow::Result;
use rusqlite::Connection;
use std::time::Duration;
use tracing::info;

#[derive(Debug, Clone)]
pub struct ZmemoryRepository {
    config: ZmemoryConfig,
}

impl ZmemoryRepository {
    pub fn new(config: ZmemoryConfig) -> Self {
        Self { config }
    }

    pub fn connect(&self) -> Result<Connection> {
        let resolution = self.config.path_resolution();
        info!(
            db_path = %resolution.db_path.display(),
            workspace_key = resolution.workspace_key.as_deref(),
            source = ?resolution.source,
            reason = %resolution.reason,
            "resolved zmemory path"
        );
        if let Some(parent) = self.config.db_path().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut conn = Connection::open(self.config.db_path())?;
        conn.busy_timeout(Duration::from_secs(5))?;
        initialize_database(&mut conn, self.config.namespace())?;
        Ok(conn)
    }
}

#[cfg(test)]
mod tests {
    use super::ZmemoryRepository;
    use crate::ZmemoryConfig;
    use crate::path_resolution::resolve_workspace_base_path;
    use crate::path_resolution::resolve_zmemory_path;
    use anyhow::Result;
    use tempfile::TempDir;
    use tracing_test::traced_test;

    #[test]
    #[traced_test]
    fn connect_logs_resolved_path_details() -> Result<()> {
        let codex_home = TempDir::new()?;
        let cwd = TempDir::new()?;
        let resolution = resolve_zmemory_path(codex_home.path(), cwd.path(), None)?;
        let workspace_base = resolve_workspace_base_path(cwd.path())?;
        let config = ZmemoryConfig::new(codex_home.path(), workspace_base, resolution.clone());

        let repository = ZmemoryRepository::new(config);
        let _conn = repository.connect()?;

        logs_assert(|lines: &[&str]| {
            lines
                .iter()
                .find(|line| {
                    line.contains("resolved zmemory path")
                        && line.contains(&resolution.db_path.display().to_string())
                        && line.contains(&resolution.reason)
                })
                .map(|_| Ok(()))
                .unwrap_or_else(|| Err("expected resolved zmemory path log".to_string()))
        });

        Ok(())
    }
}

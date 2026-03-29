use crate::config::ZmemoryConfig;
use crate::schema::initialize_database;
use anyhow::Result;
use rusqlite::Connection;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct ZmemoryRepository {
    config: ZmemoryConfig,
}

impl ZmemoryRepository {
    pub fn new(config: ZmemoryConfig) -> Self {
        Self { config }
    }

    pub fn connect(&self) -> Result<Connection> {
        if let Some(parent) = self.config.db_path().parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(self.config.db_path())?;
        conn.busy_timeout(Duration::from_secs(5))?;
        initialize_database(&conn)?;
        Ok(conn)
    }
}

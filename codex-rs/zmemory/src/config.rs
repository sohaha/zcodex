use std::path::Path;
use std::path::PathBuf;

pub const ZMEMORY_DIR: &str = "zmemory";
pub const ZMEMORY_DB_FILENAME: &str = "zmemory.db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemoryConfig {
    codex_home: PathBuf,
    db_path: PathBuf,
}

impl ZmemoryConfig {
    pub fn new(codex_home: impl Into<PathBuf>) -> Self {
        let codex_home = codex_home.into();
        let db_path = zmemory_db_path(&codex_home);
        Self {
            codex_home,
            db_path,
        }
    }

    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }
}

pub fn zmemory_db_path(codex_home: &Path) -> PathBuf {
    codex_home.join(ZMEMORY_DIR).join(ZMEMORY_DB_FILENAME)
}

#[cfg(test)]
mod tests {
    use super::ZMEMORY_DB_FILENAME;
    use super::ZMEMORY_DIR;
    use super::zmemory_db_path;

    #[test]
    fn db_path_uses_codex_home_subdirectory() {
        let codex_home = std::path::Path::new("/tmp/codex-home");
        assert_eq!(
            zmemory_db_path(codex_home),
            codex_home.join(ZMEMORY_DIR).join(ZMEMORY_DB_FILENAME)
        );
    }
}

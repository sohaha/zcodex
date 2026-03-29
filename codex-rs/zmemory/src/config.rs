use std::path::Path;
use std::path::PathBuf;

pub const ZMEMORY_DIR: &str = "zmemory";
pub const ZMEMORY_DB_FILENAME: &str = "zmemory.db";
const VALID_DOMAINS_ENV: &str = "VALID_DOMAINS";
const CORE_MEMORY_URIS_ENV: &str = "CORE_MEMORY_URIS";
const DEFAULT_VALID_DOMAINS: &[&str] = &["core"];
const DEFAULT_CORE_MEMORY_URIS: &[&str] =
    &["core://agent", "core://my_user", "core://agent/my_user"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemoryConfig {
    codex_home: PathBuf,
    db_path: PathBuf,
    settings: ZmemorySettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemorySettings {
    valid_domains: Vec<String>,
    core_memory_uris: Vec<String>,
}

impl ZmemoryConfig {
    pub fn new(codex_home: impl Into<PathBuf>) -> Self {
        Self::new_with_settings(
            codex_home,
            ZmemorySettings::from_env_vars(
                std::env::var(VALID_DOMAINS_ENV).ok(),
                std::env::var(CORE_MEMORY_URIS_ENV).ok(),
            ),
        )
    }

    pub fn new_with_settings(codex_home: impl Into<PathBuf>, settings: ZmemorySettings) -> Self {
        let codex_home = codex_home.into();
        let db_path = zmemory_db_path(&codex_home);
        Self {
            codex_home,
            db_path,
            settings,
        }
    }

    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    pub fn is_valid_domain(&self, domain: &str) -> bool {
        domain == "system"
            || self
                .settings
                .valid_domains
                .iter()
                .any(|value| value == domain)
    }

    pub fn core_memory_uris(&self) -> &[String] {
        &self.settings.core_memory_uris
    }
}

impl ZmemorySettings {
    pub fn from_env_vars(valid_domains: Option<String>, core_memory_uris: Option<String>) -> Self {
        Self {
            valid_domains: parse_csv(valid_domains.as_deref(), DEFAULT_VALID_DOMAINS),
            core_memory_uris: parse_csv(core_memory_uris.as_deref(), DEFAULT_CORE_MEMORY_URIS),
        }
    }
}

pub fn zmemory_db_path(codex_home: &Path) -> PathBuf {
    codex_home.join(ZMEMORY_DIR).join(ZMEMORY_DB_FILENAME)
}

fn parse_csv(raw: Option<&str>, defaults: &[&str]) -> Vec<String> {
    let values = raw
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_lowercase())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
        .unwrap_or_else(|| defaults.iter().map(|value| value.to_string()).collect());

    let mut deduped = Vec::new();
    for value in values {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    deduped
}

#[cfg(test)]
mod tests {
    use super::DEFAULT_CORE_MEMORY_URIS;
    use super::DEFAULT_VALID_DOMAINS;
    use super::ZMEMORY_DB_FILENAME;
    use super::ZMEMORY_DIR;
    use super::ZmemoryConfig;
    use super::ZmemorySettings;
    use super::zmemory_db_path;
    use pretty_assertions::assert_eq;

    #[test]
    fn db_path_uses_codex_home_subdirectory() {
        let codex_home = std::path::Path::new("/tmp/codex-home");
        assert_eq!(
            zmemory_db_path(codex_home),
            codex_home.join(ZMEMORY_DIR).join(ZMEMORY_DB_FILENAME)
        );
    }

    #[test]
    fn settings_default_to_core_domain_and_boot_anchors() {
        let settings = ZmemorySettings::from_env_vars(None, None);

        assert_eq!(
            settings,
            ZmemorySettings {
                valid_domains: DEFAULT_VALID_DOMAINS
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
                core_memory_uris: DEFAULT_CORE_MEMORY_URIS
                    .iter()
                    .map(|value| value.to_string())
                    .collect(),
            }
        );
    }

    #[test]
    fn settings_normalize_and_deduplicate_csv_values() {
        let settings = ZmemorySettings::from_env_vars(
            Some("core, Writer ,core,notes".to_string()),
            Some("core://agent, core://my_user ,core://agent".to_string()),
        );

        assert_eq!(
            settings,
            ZmemorySettings {
                valid_domains: vec![
                    "core".to_string(),
                    "writer".to_string(),
                    "notes".to_string(),
                ],
                core_memory_uris: vec!["core://agent".to_string(), "core://my_user".to_string(),],
            }
        );
    }

    #[test]
    fn config_allows_system_even_when_not_listed() {
        let config = ZmemoryConfig::new_with_settings(
            "/tmp/codex-home",
            ZmemorySettings::from_env_vars(Some("writer".to_string()), None),
        );

        assert!(config.is_valid_domain("writer"));
        assert!(config.is_valid_domain("system"));
        assert!(!config.is_valid_domain("core"));
    }
}

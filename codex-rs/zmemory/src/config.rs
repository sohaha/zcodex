use crate::path_resolution::ZmemoryPathResolution;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const ZMEMORY_DIR: &str = "zmemory";
pub(crate) const ZMEMORY_DB_FILENAME: &str = "zmemory.db";
const VALID_DOMAINS_ENV: &str = "VALID_DOMAINS";
const CORE_MEMORY_URIS_ENV: &str = "CORE_MEMORY_URIS";
const DEFAULT_VALID_DOMAINS: &[&str] = &["core"];
const DEFAULT_CORE_MEMORY_URIS: &[&str] =
    &["core://agent", "core://my_user", "core://agent/my_user"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemoryConfig {
    codex_home: PathBuf,
    path_resolution: ZmemoryPathResolution,
    settings: ZmemorySettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemorySettings {
    valid_domains: Vec<String>,
    core_memory_uris: Vec<String>,
}

impl ZmemoryConfig {
    pub fn new(codex_home: impl Into<PathBuf>, path_resolution: ZmemoryPathResolution) -> Self {
        Self::new_with_settings(
            codex_home,
            path_resolution,
            ZmemorySettings::from_env_vars(
                std::env::var(VALID_DOMAINS_ENV).ok(),
                std::env::var(CORE_MEMORY_URIS_ENV).ok(),
            ),
        )
    }

    pub fn new_with_settings(
        codex_home: impl Into<PathBuf>,
        path_resolution: ZmemoryPathResolution,
        settings: ZmemorySettings,
    ) -> Self {
        Self {
            codex_home: codex_home.into(),
            path_resolution,
            settings,
        }
    }

    pub fn codex_home(&self) -> &Path {
        &self.codex_home
    }

    pub fn db_path(&self) -> &Path {
        &self.path_resolution.db_path
    }

    pub fn path_resolution(&self) -> &ZmemoryPathResolution {
        &self.path_resolution
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

    pub fn valid_domains(&self) -> &[String] {
        &self.settings.valid_domains
    }

    pub fn valid_domains_for_display(&self) -> Vec<String> {
        let mut values = self.settings.valid_domains.clone();
        if !values.iter().any(|value| value == "system") {
            values.push("system".to_string());
        }
        values
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
    use crate::path_resolution::ZmemoryPathResolution;
    use crate::path_resolution::ZmemoryPathSource;
    use pretty_assertions::assert_eq;
    use std::path::PathBuf;

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
                core_memory_uris: vec!["core://agent".to_string(), "core://my_user".to_string()],
            }
        );
    }

    #[test]
    fn config_allows_system_even_when_not_listed() {
        let config = ZmemoryConfig::new_with_settings(
            "/tmp/codex-home",
            sample_resolution("/tmp/codex-home/zmemory/workspace-test/zmemory.db"),
            ZmemorySettings::from_env_vars(Some("writer".to_string()), None),
        );

        assert!(config.is_valid_domain("writer"));
        assert!(config.is_valid_domain("system"));
        assert!(!config.is_valid_domain("core"));
    }

    #[test]
    fn config_uses_resolved_db_path() {
        let resolution = sample_resolution("/tmp/workspace/memory.db");
        let config = ZmemoryConfig::new_with_settings(
            "/tmp/codex-home",
            resolution.clone(),
            ZmemorySettings::from_env_vars(None, None),
        );

        assert_eq!(config.db_path(), resolution.db_path.as_path());
        assert_eq!(config.path_resolution(), &resolution);
    }

    fn sample_resolution(db_path: &str) -> ZmemoryPathResolution {
        ZmemoryPathResolution {
            db_path: PathBuf::from(db_path),
            workspace_key: Some("workspace-test".to_string()),
            source: ZmemoryPathSource::Cwd,
            canonical_base: Some(PathBuf::from("/tmp/workspace")),
            reason: "test".to_string(),
        }
    }
}

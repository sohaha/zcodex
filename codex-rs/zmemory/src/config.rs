use crate::path_resolution::ZmemoryPathResolution;
use sha2::Digest;
use sha2::Sha256;
use std::path::Path;
use std::path::PathBuf;

pub(crate) const ZMEMORY_DIR: &str = "zmemory";
pub(crate) const ZMEMORY_PROJECTS_DIR: &str = "projects";
pub(crate) const ZMEMORY_DB_FILENAME: &str = "zmemory.db";
const VALID_DOMAINS_ENV: &str = "VALID_DOMAINS";
const CORE_MEMORY_URIS_ENV: &str = "CORE_MEMORY_URIS";
const DEFAULT_VALID_DOMAINS: &[&str] = &["core"];
const DEFAULT_CORE_MEMORY_URIS: &[&str] =
    &["core://agent", "core://my_user", "core://agent/my_user"];

pub(crate) fn default_valid_domains() -> &'static [&'static str] {
    DEFAULT_VALID_DOMAINS
}

pub(crate) fn default_core_memory_uris() -> &'static [&'static str] {
    DEFAULT_CORE_MEMORY_URIS
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemoryConfig {
    codex_home: PathBuf,
    workspace_base: PathBuf,
    path_resolution: ZmemoryPathResolution,
    settings: ZmemorySettings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemorySettings {
    valid_domains: Vec<String>,
    core_memory_uris: Vec<String>,
}

impl ZmemoryConfig {
    pub fn new(
        codex_home: impl Into<PathBuf>,
        workspace_base: impl Into<PathBuf>,
        path_resolution: ZmemoryPathResolution,
    ) -> Self {
        Self::new_with_settings(
            codex_home,
            workspace_base,
            path_resolution,
            ZmemorySettings::from_config_over_env(None, None),
        )
    }

    pub fn new_with_settings(
        codex_home: impl Into<PathBuf>,
        workspace_base: impl Into<PathBuf>,
        path_resolution: ZmemoryPathResolution,
        settings: ZmemorySettings,
    ) -> Self {
        Self {
            codex_home: codex_home.into(),
            workspace_base: workspace_base.into(),
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

    pub fn workspace_base(&self) -> &Path {
        &self.workspace_base
    }

    pub fn path_resolution(&self) -> &ZmemoryPathResolution {
        &self.path_resolution
    }

    pub fn is_valid_domain(&self, domain: &str) -> bool {
        domain == "system"
            || domain == "alias"
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
        if !values.iter().any(|value| value == "alias") {
            values.push("alias".to_string());
        }
        values
    }
}

impl ZmemorySettings {
    pub fn from_config_over_env(
        valid_domains: Option<Vec<String>>,
        core_memory_uris: Option<Vec<String>>,
    ) -> Self {
        Self::from_sources(
            valid_domains,
            core_memory_uris,
            std::env::var(VALID_DOMAINS_ENV).ok(),
            std::env::var(CORE_MEMORY_URIS_ENV).ok(),
        )
    }

    pub fn from_env_vars(valid_domains: Option<String>, core_memory_uris: Option<String>) -> Self {
        Self::from_sources(None, None, valid_domains, core_memory_uris)
    }

    pub fn from_sources(
        valid_domains: Option<Vec<String>>,
        core_memory_uris: Option<Vec<String>>,
        env_valid_domains: Option<String>,
        env_core_memory_uris: Option<String>,
    ) -> Self {
        Self {
            valid_domains: parse_setting_values(
                valid_domains.as_deref(),
                env_valid_domains.as_deref(),
                DEFAULT_VALID_DOMAINS,
            ),
            core_memory_uris: parse_setting_values(
                core_memory_uris.as_deref(),
                env_core_memory_uris.as_deref(),
                DEFAULT_CORE_MEMORY_URIS,
            ),
        }
    }
}

pub fn global_zmemory_db_path(codex_home: &Path) -> PathBuf {
    codex_home.join(ZMEMORY_DIR).join(ZMEMORY_DB_FILENAME)
}

pub fn project_key_for_workspace(workspace_base: &Path) -> String {
    let workspace_label = workspace_base
        .file_name()
        .and_then(|value| value.to_str())
        .map(sanitize_project_slug)
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "workspace".to_string());

    let digest = Sha256::digest(workspace_base.to_string_lossy().as_bytes());
    let hash = format!("{digest:x}");
    format!("{workspace_label}-{}", &hash[..12])
}

pub fn zmemory_db_path(codex_home: &Path, workspace_base: &Path) -> PathBuf {
    codex_home
        .join(ZMEMORY_DIR)
        .join(ZMEMORY_PROJECTS_DIR)
        .join(project_key_for_workspace(workspace_base))
        .join(ZMEMORY_DB_FILENAME)
}

fn sanitize_project_slug(raw: &str) -> String {
    let mut slug = String::with_capacity(raw.len());
    let mut previous_was_separator = false;
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_was_separator = false;
        } else if !previous_was_separator {
            slug.push('-');
            previous_was_separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    slug.trim_start_matches('-').to_string()
}

fn parse_setting_values(
    configured: Option<&[String]>,
    env_raw: Option<&str>,
    defaults: &[&str],
) -> Vec<String> {
    configured
        .map(|values| normalize_values(values.iter().map(String::as_str)))
        .filter(|values| !values.is_empty())
        .or_else(|| {
            env_raw
                .map(|value| {
                    normalize_values(
                        value
                            .split(',')
                            .map(str::trim)
                            .filter(|value| !value.is_empty()),
                    )
                })
                .filter(|values| !values.is_empty())
        })
        .unwrap_or_else(|| normalize_values(defaults.iter().copied()))
}

fn normalize_values<'a>(values: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        let normalized = value.trim().to_lowercase();
        if !normalized.is_empty() && !deduped.contains(&normalized) {
            deduped.push(normalized);
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
    use super::ZMEMORY_PROJECTS_DIR;
    use super::ZmemoryConfig;
    use super::ZmemorySettings;
    use super::global_zmemory_db_path;
    use super::project_key_for_workspace;
    use super::zmemory_db_path;
    use crate::path_resolution::ZmemoryPathResolution;
    use crate::path_resolution::ZmemoryPathSource;
    use pretty_assertions::assert_eq;
    use std::path::Path;
    use std::path::PathBuf;

    #[test]
    fn db_path_uses_project_subdirectory() {
        let codex_home = std::path::Path::new("/tmp/codex-home");
        let workspace_base = std::path::Path::new("/tmp/workspace/demo-repo");
        assert_eq!(
            zmemory_db_path(codex_home, workspace_base),
            codex_home
                .join(ZMEMORY_DIR)
                .join(ZMEMORY_PROJECTS_DIR)
                .join(project_key_for_workspace(workspace_base))
                .join(ZMEMORY_DB_FILENAME)
        );
    }

    #[test]
    fn global_db_path_still_uses_legacy_root_location() {
        let codex_home = std::path::Path::new("/tmp/codex-home");
        assert_eq!(
            global_zmemory_db_path(codex_home),
            codex_home.join(ZMEMORY_DIR).join(ZMEMORY_DB_FILENAME)
        );
    }

    #[test]
    fn project_key_uses_slug_and_hash() {
        let key = project_key_for_workspace(Path::new("/tmp/Workspace Demo"));
        assert!(key.starts_with("workspace-demo-"));
        assert_eq!(key.len(), "workspace-demo-".len() + 12);
    }

    #[test]
    fn settings_default_to_core_domain_and_boot_anchors() {
        let settings = ZmemorySettings::from_env_vars(None, None);

        assert_eq!(
            settings,
            ZmemorySettings {
                valid_domains: DEFAULT_VALID_DOMAINS
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
                core_memory_uris: DEFAULT_CORE_MEMORY_URIS
                    .iter()
                    .map(std::string::ToString::to_string)
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
    fn settings_prefer_config_values_over_env() {
        let settings = ZmemorySettings::from_sources(
            Some(vec![
                "core".to_string(),
                "project".to_string(),
                "CORE".to_string(),
            ]),
            Some(vec![
                "core://agent/coding_operating_manual".to_string(),
                "core://my_user/coding_preferences".to_string(),
            ]),
            Some("writer,notes".to_string()),
            Some("core://agent,core://my_user".to_string()),
        );

        assert_eq!(
            settings,
            ZmemorySettings {
                valid_domains: vec!["core".to_string(), "project".to_string()],
                core_memory_uris: vec![
                    "core://agent/coding_operating_manual".to_string(),
                    "core://my_user/coding_preferences".to_string(),
                ],
            }
        );
    }

    #[test]
    fn config_allows_system_even_when_not_listed() {
        let config = ZmemoryConfig::new_with_settings(
            "/tmp/codex-home",
            "/tmp/workspace",
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
            "/tmp/workspace",
            resolution.clone(),
            ZmemorySettings::from_env_vars(None, None),
        );

        assert_eq!(config.db_path(), resolution.db_path.as_path());
        assert_eq!(config.workspace_base(), Path::new("/tmp/workspace"));
        assert_eq!(config.path_resolution(), &resolution);
    }

    fn sample_resolution(db_path: &str) -> ZmemoryPathResolution {
        ZmemoryPathResolution {
            db_path: PathBuf::from(db_path),
            workspace_key: None,
            source: ZmemoryPathSource::ProjectScoped,
            canonical_base: None,
            reason: "test".to_string(),
        }
    }
}

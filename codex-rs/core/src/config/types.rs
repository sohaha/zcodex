use codex_utils_absolute_path::AbsolutePathBuf;
use codex_zmemory::config::ZmemorySettings;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

/// Zmemory settings loaded from config.toml.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ZmemoryToml {
    /// Optional override for the `zmemory` database path.
    pub path: Option<AbsolutePathBuf>,
    /// Optional writable memory domains for the current runtime profile.
    pub valid_domains: Option<Vec<String>>,
    /// Optional boot anchor URIs for the current runtime profile.
    pub core_memory_uris: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemoryConfig {
    pub path: Option<PathBuf>,
    valid_domains: Option<Vec<String>>,
    core_memory_uris: Option<Vec<String>>,
}

impl ZmemoryConfig {
    pub fn from_toml(toml: Option<ZmemoryToml>) -> Self {
        let (path, valid_domains, core_memory_uris) = match toml {
            Some(toml) => (
                toml.path.map(AbsolutePathBuf::into_path_buf),
                toml.valid_domains,
                toml.core_memory_uris,
            ),
            None => (None, None, None),
        };
        Self {
            path,
            valid_domains,
            core_memory_uris,
        }
    }

    pub fn to_runtime_settings(&self) -> ZmemorySettings {
        ZmemorySettings::from_config_over_env(
            self.valid_domains.clone(),
            self.core_memory_uris.clone(),
        )
    }
}

impl Default for ZmemoryConfig {
    fn default() -> Self {
        Self::from_toml(None)
    }
}

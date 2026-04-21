use codex_config::types::ZtokBehavior;
use codex_config::types::ZtokToml;
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
    ///
    /// Keep this as a raw `PathBuf` so relative paths can be resolved later by
    /// the zmemory path resolver against the repo root or cwd, rather than
    /// being forced through config-layer `AbsolutePathBuf` resolution.
    pub path: Option<PathBuf>,
    /// Optional writable memory domains for the current runtime profile.
    pub valid_domains: Option<Vec<String>>,
    /// Optional boot anchor URIs for the current runtime profile.
    pub core_memory_uris: Option<Vec<String>>,
    /// Optional namespace override for the current runtime profile.
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZmemoryConfig {
    pub path: Option<PathBuf>,
    valid_domains: Option<Vec<String>>,
    core_memory_uris: Option<Vec<String>>,
    namespace: Option<String>,
}

impl ZmemoryConfig {
    pub fn from_toml(toml: Option<ZmemoryToml>) -> Self {
        let (path, valid_domains, core_memory_uris, namespace) = match toml {
            Some(toml) => (
                toml.path,
                toml.valid_domains,
                toml.core_memory_uris,
                toml.namespace,
            ),
            None => (None, None, None, None),
        };
        Self {
            path,
            valid_domains,
            core_memory_uris,
            namespace,
        }
    }

    pub fn to_runtime_settings(&self) -> ZmemorySettings {
        ZmemorySettings::from_config_over_env(
            self.valid_domains.clone(),
            self.core_memory_uris.clone(),
        )
        .with_namespace(self.namespace.clone())
    }
}

impl Default for ZmemoryConfig {
    fn default() -> Self {
        Self::from_toml(None)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZtokConfig {
    pub behavior: ZtokBehavior,
}

impl ZtokConfig {
    pub fn from_toml(toml: Option<ZtokToml>) -> Self {
        Self {
            behavior: toml.and_then(|config| config.behavior).unwrap_or_default(),
        }
    }
}

impl Default for ZtokConfig {
    fn default() -> Self {
        Self::from_toml(None)
    }
}

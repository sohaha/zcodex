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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct ContextHooksToml {
    pub enabled: Option<bool>,
    pub snapshot_token_budget: Option<usize>,
    pub max_events_per_snapshot: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextHooksConfig {
    pub enabled: bool,
    pub snapshot_token_budget: usize,
    pub max_events_per_snapshot: usize,
}

impl ContextHooksConfig {
    pub fn from_toml(toml: Option<ContextHooksToml>) -> Self {
        let defaults = codex_context_hooks::ContextHooksSettings::default();
        let Some(toml) = toml else {
            return Self {
                enabled: true,
                snapshot_token_budget: defaults.snapshot_token_budget,
                max_events_per_snapshot: defaults.max_events_per_snapshot,
            };
        };
        Self {
            enabled: toml.enabled.unwrap_or(true),
            snapshot_token_budget: toml
                .snapshot_token_budget
                .unwrap_or(defaults.snapshot_token_budget),
            max_events_per_snapshot: toml
                .max_events_per_snapshot
                .unwrap_or(defaults.max_events_per_snapshot),
        }
    }

    pub fn to_context_hooks_settings(&self) -> codex_context_hooks::ContextHooksSettings {
        codex_context_hooks::ContextHooksSettings {
            snapshot_token_budget: self.snapshot_token_budget,
            max_events_per_snapshot: self.max_events_per_snapshot,
        }
    }
}

impl Default for ContextHooksConfig {
    fn default() -> Self {
        Self::from_toml(None)
    }
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

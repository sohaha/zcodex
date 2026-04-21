use crate::behavior::ZTOK_BEHAVIOR_ENV_VAR;
use crate::behavior::ZtokBehavior;
use crate::near_dedup::NearDuplicateConfig;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

pub const ZTOK_RUNTIME_SETTINGS_ENV_VAR: &str = "CODEX_ZTOK_RUNTIME_SETTINGS";
pub const ZTOK_SESSION_ID_ENV_VAR: &str = "CODEX_ZTOK_SESSION_ID";

#[derive(Debug, Clone)]
pub(crate) struct ZtokRuntimeSettings {
    pub behavior: ZtokBehavior,
    pub session_cache: SessionCacheSettings,
    pub near_dedup: NearDedupSettings,
    pub decision_trace: DecisionTraceSettings,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionCacheSettings {
    pub db_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct NearDedupSettings {
    pub text: NearDuplicateConfig,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DecisionTraceSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ZtokRuntimeSettingsPayload {
    #[serde(default)]
    behavior: Option<String>,
    #[serde(default)]
    session_cache: SessionCachePayload,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SessionCachePayload {
    #[serde(default)]
    session_id: Option<String>,
}

pub(crate) fn runtime_settings() -> ZtokRuntimeSettings {
    ZtokRuntimeSettings::from_env()
}

pub fn encode_runtime_settings_env(
    session_id: Option<&str>,
    behavior: &str,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&ZtokRuntimeSettingsPayload {
        behavior: Some(behavior.to_string()),
        session_cache: SessionCachePayload {
            session_id: sanitize_optional_string(session_id),
        },
    })
}

impl ZtokRuntimeSettings {
    fn from_env() -> Self {
        let payload = std::env::var(ZTOK_RUNTIME_SETTINGS_ENV_VAR)
            .ok()
            .and_then(|raw| serde_json::from_str::<ZtokRuntimeSettingsPayload>(&raw).ok())
            .unwrap_or_else(Self::legacy_payload);
        Self::from_payload(payload)
    }

    fn legacy_payload() -> ZtokRuntimeSettingsPayload {
        ZtokRuntimeSettingsPayload {
            behavior: std::env::var(ZTOK_BEHAVIOR_ENV_VAR).ok(),
            session_cache: SessionCachePayload {
                session_id: std::env::var(ZTOK_SESSION_ID_ENV_VAR).ok(),
            },
        }
    }

    fn from_payload(payload: ZtokRuntimeSettingsPayload) -> Self {
        let session_id = sanitize_owned_string(payload.session_cache.session_id);
        let db_path = session_id
            .as_deref()
            .and_then(session_cache_path_for_session_id);
        Self {
            behavior: ZtokBehavior::from_value(payload.behavior.as_deref()),
            session_cache: SessionCacheSettings { db_path },
            near_dedup: NearDedupSettings {
                text: NearDuplicateConfig::default(),
            },
            decision_trace: DecisionTraceSettings::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_test(
        behavior: ZtokBehavior,
        cache_path: Option<PathBuf>,
        near_duplicate_config: NearDuplicateConfig,
    ) -> Self {
        Self {
            behavior,
            session_cache: SessionCacheSettings {
                db_path: cache_path,
            },
            near_dedup: NearDedupSettings {
                text: near_duplicate_config,
            },
            decision_trace: DecisionTraceSettings::default(),
        }
    }
}

pub(crate) fn session_cache_path_for_session_id(session_id: &str) -> Option<PathBuf> {
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))?;
    Some(
        codex_home
            .join(".ztok-cache")
            .join(format!("{session_id}.sqlite")),
    )
}

fn sanitize_optional_string(value: Option<&str>) -> Option<String> {
    sanitize_owned_string(value.map(ToOwned::to_owned))
}

fn sanitize_owned_string(value: Option<String>) -> Option<String> {
    value.and_then(|candidate| {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_payload_round_trips_to_runtime_settings() {
        let raw = encode_runtime_settings_env(Some("thread-1"), "basic")
            .expect("encode runtime settings payload");
        let payload: ZtokRuntimeSettingsPayload =
            serde_json::from_str(&raw).expect("decode runtime settings payload");
        let settings = ZtokRuntimeSettings::from_payload(payload);

        assert!(settings.behavior.is_basic());
        assert!(
            settings
                .session_cache
                .db_path
                .as_ref()
                .is_some_and(|path| path.ends_with("thread-1.sqlite"))
        );
    }

    #[test]
    fn session_cache_path_uses_codex_home() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        unsafe {
            std::env::set_var("CODEX_HOME", temp.path());
        }
        let path =
            session_cache_path_for_session_id("thread-2").expect("session cache path available");
        assert!(path.starts_with(temp.path()));
        assert!(path.ends_with(".ztok-cache/thread-2.sqlite"));
        unsafe {
            std::env::remove_var("CODEX_HOME");
        }
    }
}

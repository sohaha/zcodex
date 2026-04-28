use crate::behavior::ZTOK_BEHAVIOR_ENV_VAR;
use crate::behavior::ZtokBehavior;
use crate::near_dedup::NearDuplicateConfig;
use serde::Deserialize;
use serde::Serialize;
use std::path::PathBuf;

pub const ZTOK_RUNTIME_SETTINGS_ENV_VAR: &str = "CODEX_ZTOK_RUNTIME_SETTINGS";
pub const ZTOK_SESSION_ID_ENV_VAR: &str = "CODEX_ZTOK_SESSION_ID";
pub const ZTOK_NO_DEDUP_ENV_VAR: &str = "CODEX_ZTOK_NO_DEDUP";

#[derive(Debug, Clone)]
pub(crate) struct ZtokRuntimeSettings {
    pub behavior: ZtokBehavior,
    pub session_cache: SessionCacheSettings,
    pub near_dedup: NearDedupSettings,
    pub decision_trace: DecisionTraceSettings,
    pub no_cache: NoCacheSettings,
}

#[derive(Debug, Clone)]
pub(crate) struct SessionCacheSettings {
    pub session_id: Option<String>,
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

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct NoCacheSettings {
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct ZtokRuntimeSettingsPayload {
    #[serde(default)]
    behavior: Option<String>,
    #[serde(default)]
    session_cache: SessionCachePayload,
    #[serde(default)]
    decision_trace: DecisionTracePayload,
    #[serde(default)]
    no_cache: NoCachePayload,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SessionCachePayload {
    #[serde(default)]
    session_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DecisionTracePayload {
    #[serde(default)]
    enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct NoCachePayload {
    #[serde(default)]
    enabled: Option<bool>,
}

pub(crate) fn runtime_settings() -> ZtokRuntimeSettings {
    ZtokRuntimeSettings::from_env()
}

pub fn encode_runtime_settings_env(
    session_id: Option<&str>,
    behavior: &str,
) -> Result<String, serde_json::Error> {
    serde_json::to_string(&runtime_settings_payload(
        session_id, behavior, /*decision_trace_enabled*/ false,
        /*no_cache_enabled*/ false,
    ))
}

pub(crate) fn apply_decision_trace_override(enabled: bool) -> Result<(), serde_json::Error> {
    let mut payload = std::env::var(ZTOK_RUNTIME_SETTINGS_ENV_VAR)
        .ok()
        .and_then(|raw| serde_json::from_str::<ZtokRuntimeSettingsPayload>(&raw).ok())
        .unwrap_or_else(ZtokRuntimeSettings::legacy_payload);
    payload.decision_trace.enabled = Some(enabled);
    unsafe {
        std::env::set_var(
            ZTOK_RUNTIME_SETTINGS_ENV_VAR,
            serde_json::to_string(&payload)?,
        );
    }
    Ok(())
}

pub(crate) fn apply_no_cache_override(enabled: bool) -> Result<(), serde_json::Error> {
    let mut payload = std::env::var(ZTOK_RUNTIME_SETTINGS_ENV_VAR)
        .ok()
        .and_then(|raw| serde_json::from_str::<ZtokRuntimeSettingsPayload>(&raw).ok())
        .unwrap_or_else(ZtokRuntimeSettings::legacy_payload);
    payload.no_cache.enabled = Some(enabled);
    unsafe {
        std::env::set_var(
            ZTOK_RUNTIME_SETTINGS_ENV_VAR,
            serde_json::to_string(&payload)?,
        );
    }
    Ok(())
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
            decision_trace: DecisionTracePayload::default(),
            no_cache: NoCachePayload {
                enabled: Some(no_dedup_env_enabled()),
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
            session_cache: SessionCacheSettings {
                session_id,
                db_path,
            },
            near_dedup: NearDedupSettings {
                text: NearDuplicateConfig::default(),
            },
            decision_trace: DecisionTraceSettings {
                enabled: payload.decision_trace.enabled.unwrap_or(false),
            },
            no_cache: NoCacheSettings {
                enabled: payload.no_cache.enabled.unwrap_or(false) || no_dedup_env_enabled(),
            },
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
                session_id: None,
                db_path: cache_path,
            },
            near_dedup: NearDedupSettings {
                text: near_duplicate_config,
            },
            decision_trace: DecisionTraceSettings::default(),
            no_cache: NoCacheSettings::default(),
        }
    }
}

fn runtime_settings_payload(
    session_id: Option<&str>,
    behavior: &str,
    decision_trace_enabled: bool,
    no_cache_enabled: bool,
) -> ZtokRuntimeSettingsPayload {
    ZtokRuntimeSettingsPayload {
        behavior: Some(behavior.to_string()),
        session_cache: SessionCachePayload {
            session_id: sanitize_optional_string(session_id),
        },
        decision_trace: DecisionTracePayload {
            enabled: Some(decision_trace_enabled),
        },
        no_cache: NoCachePayload {
            enabled: Some(no_cache_enabled),
        },
    }
}

fn no_dedup_env_enabled() -> bool {
    std::env::var(ZTOK_NO_DEDUP_ENV_VAR)
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "on"))
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
        assert!(!settings.decision_trace.enabled);
        assert!(!settings.no_cache.enabled);
        assert_eq!(
            settings.session_cache.session_id.as_deref(),
            Some("thread-1")
        );
        assert!(
            settings
                .session_cache
                .db_path
                .as_ref()
                .is_some_and(|path| path.ends_with("thread-1.sqlite"))
        );
    }

    #[test]
    fn decision_trace_payload_enables_runtime_trace() {
        let settings = ZtokRuntimeSettings::from_payload(runtime_settings_payload(
            Some("thread-3"),
            "enhanced",
            /*decision_trace_enabled*/ true,
            /*no_cache_enabled*/ false,
        ));

        assert!(settings.decision_trace.enabled);
    }

    #[test]
    fn no_cache_payload_disables_session_dedup() {
        let settings = ZtokRuntimeSettings::from_payload(runtime_settings_payload(
            Some("thread-4"),
            "enhanced",
            /*decision_trace_enabled*/ false,
            /*no_cache_enabled*/ true,
        ));

        assert!(settings.no_cache.enabled);
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

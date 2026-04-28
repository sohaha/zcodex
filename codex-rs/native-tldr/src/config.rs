use crate::daemon::DaemonConfig;
use crate::semantic::SemanticConfig;
use crate::session::SessionConfig;
use crate::TldrConfig;
use crate::ZtldrArtifactLocation;
use crate::ZtldrConfig;
use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

const CONFIG_TOML_FILE: &str = "config.toml";

#[derive(Debug, Default, Deserialize)]
pub struct TldrConfigFile {
    pub daemon: Option<TldrDaemonConfigFile>,
    pub semantic: Option<TldrSemanticConfigFile>,
    pub session: Option<TldrSessionConfigFile>,
}

#[derive(Debug, Default, Deserialize)]
struct GlobalConfigFile {
    pub ztldr: Option<GlobalZtldrConfigFile>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalZtldrConfigFile {
    pub enabled: Option<bool>,
    pub artifact_location: Option<ZtldrArtifactLocation>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TldrDaemonConfigFile {
    pub auto_start: Option<bool>,
    pub socket_mode: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TldrSemanticConfigFile {
    pub enabled: Option<bool>,
    pub model: Option<String>,
    pub auto_reindex_threshold: Option<usize>,
    pub ignore: Option<Vec<String>>,
    pub embedding: Option<TldrSemanticEmbeddingConfigFile>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TldrSemanticEmbeddingConfigFile {
    pub enabled: Option<bool>,
    pub dimensions: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
pub struct TldrSessionConfigFile {
    pub dirty_file_threshold: Option<usize>,
    pub idle_timeout_secs: Option<u64>,
}

pub fn load_tldr_config(project_root: &Path) -> Result<TldrConfig> {
    let codex_home = default_codex_home();
    load_tldr_config_with_codex_home(project_root, codex_home.as_deref())
}

fn load_tldr_config_with_codex_home(
    project_root: &Path,
    codex_home: Option<&Path>,
) -> Result<TldrConfig> {
    let mut config = TldrConfig::for_project(project_root.to_path_buf());
    config.ztldr = load_global_ztldr_config_from_codex_home(codex_home)?;

    let config_path = project_root.join(".codex").join("tldr.toml");
    if !config_path.exists() {
        return Ok(config);
    }

    let file = std::fs::read_to_string(&config_path)
        .with_context(|| format!("read tldr config {}", config_path.display()))?;
    let parsed: TldrConfigFile = toml::from_str(&file)
        .with_context(|| format!("parse tldr config {}", config_path.display()))?;

    if let Some(daemon) = parsed.daemon {
        apply_daemon_config(&mut config.daemon, daemon);
    }
    if let Some(semantic) = parsed.semantic {
        apply_semantic_config(&mut config.semantic, semantic);
    }
    if let Some(session) = parsed.session {
        apply_session_config(&mut config.session, session);
    }

    Ok(config)
}

pub(crate) fn load_global_ztldr_config() -> Result<ZtldrConfig> {
    let codex_home = default_codex_home();
    load_global_ztldr_config_from_codex_home(codex_home.as_deref())
}

fn load_global_ztldr_config_from_codex_home(codex_home: Option<&Path>) -> Result<ZtldrConfig> {
    let Some(codex_home) = codex_home else {
        return Ok(ZtldrConfig::default());
    };
    let config_path = codex_home.join(CONFIG_TOML_FILE);
    if !config_path.exists() {
        return Ok(ZtldrConfig::default());
    }

    let file = std::fs::read_to_string(&config_path)
        .with_context(|| format!("read Codex config {}", config_path.display()))?;
    let parsed: GlobalConfigFile = toml::from_str(&file)
        .with_context(|| format!("parse Codex config {}", config_path.display()))?;

    let ztldr = parsed.ztldr.unwrap_or_default();
    Ok(ZtldrConfig {
        enabled: ztldr.enabled.unwrap_or(false),
        artifact_location: ztldr.artifact_location.unwrap_or_default(),
    })
}

fn default_codex_home() -> Option<PathBuf> {
    if let Some(codex_home) = std::env::var_os("CODEX_HOME") {
        return Some(PathBuf::from(codex_home));
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .map(|home| home.join(".codex"))
}

fn apply_daemon_config(config: &mut DaemonConfig, file: TldrDaemonConfigFile) {
    if let Some(auto_start) = file.auto_start {
        config.auto_start = auto_start;
    }
    if let Some(socket_mode) = file.socket_mode {
        config.socket_mode = socket_mode;
    }
}

fn apply_semantic_config(config: &mut SemanticConfig, file: TldrSemanticConfigFile) {
    if let Some(enabled) = file.enabled {
        config.enabled = enabled;
    }
    if let Some(model) = file.model {
        config.model = model;
    }
    if let Some(auto_reindex_threshold) = file.auto_reindex_threshold {
        config.auto_reindex_threshold = auto_reindex_threshold;
    }
    if let Some(ignore) = file.ignore {
        config.ignore = ignore;
    }
    if let Some(embedding) = file.embedding {
        if let Some(enabled) = embedding.enabled {
            config.embedding.enabled = enabled;
            config.embedding_enabled = enabled;
        }
        if let Some(dimensions) = embedding.dimensions {
            config.embedding.dimensions = dimensions;
        }
    }
}

fn apply_session_config(config: &mut SessionConfig, file: TldrSessionConfigFile) {
    if let Some(dirty_file_threshold) = file.dirty_file_threshold {
        config.dirty_file_threshold = dirty_file_threshold;
    }
    if let Some(idle_timeout_secs) = file.idle_timeout_secs {
        config.idle_timeout = Duration::from_secs(idle_timeout_secs);
    }
}

#[cfg(test)]
mod tests {
    use super::load_global_ztldr_config_from_codex_home;
    use super::load_tldr_config_with_codex_home;
    use crate::ZtldrArtifactLocation;
    use crate::ZtldrConfig;
    use pretty_assertions::assert_eq;
    use tempfile::tempdir;

    #[test]
    fn load_tldr_config_uses_defaults_when_file_is_missing() {
        let tempdir = tempdir().expect("tempdir should exist");
        let config =
            load_tldr_config_with_codex_home(tempdir.path(), None).expect("config should load");

        assert!(config.daemon.auto_start);
        assert_eq!(config.daemon.socket_mode, "auto");
        assert!(config.semantic.enabled);
        assert_eq!(config.semantic.auto_reindex_threshold, 20);
        assert!(config.semantic.embedding.enabled);
        assert!(config.semantic.embedding_enabled);
        assert_eq!(config.session.dirty_file_threshold, 20);
        assert_eq!(config.ztldr, ZtldrConfig::default());
    }

    #[test]
    fn load_global_ztldr_config_defaults_to_disabled_temp_artifacts() {
        let codex_home = tempdir().expect("codex home should exist");

        let config = load_global_ztldr_config_from_codex_home(Some(codex_home.path()))
            .expect("global ztldr config should load");

        assert_eq!(config, ZtldrConfig::default());
        assert!(!config.uses_project_artifacts());
    }

    #[test]
    fn load_global_ztldr_config_reads_enabled_and_artifact_location() {
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nenabled = true\nartifact_location = \"project\"\n",
        )
        .expect("global config should write");

        let config = load_global_ztldr_config_from_codex_home(Some(codex_home.path()))
            .expect("global ztldr config should load");

        assert_eq!(
            config,
            ZtldrConfig {
                enabled: true,
                artifact_location: ZtldrArtifactLocation::Project,
            }
        );
        assert!(config.uses_project_artifacts());
    }

    #[test]
    fn load_global_ztldr_config_enabled_gate_controls_project_artifacts() {
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nenabled = false\nartifact_location = \"project\"\n",
        )
        .expect("global config should write");

        let config = load_global_ztldr_config_from_codex_home(Some(codex_home.path()))
            .expect("global ztldr config should load");

        assert_eq!(config.artifact_location, ZtldrArtifactLocation::Project);
        assert!(!config.uses_project_artifacts());
    }

    #[test]
    fn load_tldr_config_applies_global_ztldr_settings() {
        let project = tempdir().expect("project should exist");
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nenabled = true\nartifact_location = \"project\"\n",
        )
        .expect("global config should write");

        let config = load_tldr_config_with_codex_home(project.path(), Some(codex_home.path()))
            .expect("config should load");

        assert!(config.ztldr.enabled);
        assert_eq!(
            config.ztldr.artifact_location,
            ZtldrArtifactLocation::Project
        );
    }

    #[test]
    fn load_tldr_config_applies_overrides() {
        let tempdir = tempdir().expect("tempdir should exist");
        let codex_dir = tempdir.path().join(".codex");
        std::fs::create_dir(&codex_dir).expect("config dir should exist");
        std::fs::write(
            codex_dir.join("tldr.toml"),
            r#"
[daemon]
auto_start = false
socket_mode = "manual"

[semantic]
enabled = true
model = "jina-code"
auto_reindex_threshold = 3
ignore = ["generated.rs"]

[semantic.embedding]
enabled = true
dimensions = 128

[session]
dirty_file_threshold = 5
idle_timeout_secs = 42
"#,
        )
        .expect("config file should write");

        let config =
            load_tldr_config_with_codex_home(tempdir.path(), None).expect("config should load");
        assert!(!config.daemon.auto_start);
        assert_eq!(config.daemon.socket_mode, "manual");
        assert!(config.semantic.enabled);
        assert_eq!(config.semantic.model, "jina-code");
        assert_eq!(config.semantic.auto_reindex_threshold, 3);
        assert_eq!(config.semantic.ignore, vec!["generated.rs".to_string()]);
        assert!(config.semantic.embedding.enabled);
        assert!(config.semantic.embedding_enabled);
        assert_eq!(config.semantic.embedding.dimensions, 128);
        assert_eq!(config.session.dirty_file_threshold, 5);
        assert_eq!(config.session.idle_timeout.as_secs(), 42);
    }
}

use crate::TldrConfig;
use crate::ZtldrArtifactLocation;
use crate::ZtldrConfig;
use crate::daemon::DaemonConfig;
use crate::semantic::SemanticConfig;
use crate::session::SessionConfig;
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

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct GlobalZtldrConfig {
    pub enabled: Option<bool>,
    pub artifact_location: Option<ZtldrArtifactLocation>,
    pub onnxruntime: Option<bool>,
    pub model: Option<String>,
}

impl GlobalZtldrConfig {
    fn into_runtime_or_default(self) -> ZtldrConfig {
        ZtldrConfig {
            enabled: self.enabled.unwrap_or(false),
            artifact_location: self.artifact_location.unwrap_or_default(),
            onnxruntime: self.onnxruntime.unwrap_or(true),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct GlobalZtldrConfigFile {
    pub enabled: Option<bool>,
    pub artifact_location: Option<ZtldrArtifactLocation>,
    pub onnxruntime: Option<bool>,
    pub model: Option<String>,
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
    apply_global_ztldr_config(
        &mut config,
        load_global_ztldr_config_from_codex_home(codex_home)?,
    );

    let project_codex_config_path = project_root.join(".codex").join(CONFIG_TOML_FILE);
    apply_global_ztldr_config(
        &mut config,
        load_global_ztldr_config_from_file(&project_codex_config_path)?,
    );

    let config_path = project_root.join(".codex").join("tldr.toml");
    if !config_path.exists() {
        apply_global_ztldr_runtime_config(&mut config);
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
    apply_global_ztldr_runtime_config(&mut config);

    Ok(config)
}

pub fn load_global_ztldr_config() -> Result<ZtldrConfig> {
    let codex_home = default_codex_home();
    load_global_ztldr_config_from_codex_home(codex_home.as_deref())
        .map(GlobalZtldrConfig::into_runtime_or_default)
}

fn load_global_ztldr_config_from_codex_home(
    codex_home: Option<&Path>,
) -> Result<GlobalZtldrConfig> {
    let Some(codex_home) = codex_home else {
        return Ok(GlobalZtldrConfig::default());
    };
    load_global_ztldr_config_from_file(&codex_home.join(CONFIG_TOML_FILE))
}

fn load_global_ztldr_config_from_file(config_path: &Path) -> Result<GlobalZtldrConfig> {
    if !config_path.exists() {
        return Ok(GlobalZtldrConfig::default());
    }

    let file = std::fs::read_to_string(config_path)
        .with_context(|| format!("read Codex config {}", config_path.display()))?;
    let parsed: GlobalConfigFile = toml::from_str(&file)
        .with_context(|| format!("parse Codex config {}", config_path.display()))?;

    let ztldr = parsed.ztldr.unwrap_or_default();
    Ok(GlobalZtldrConfig {
        enabled: ztldr.enabled,
        artifact_location: ztldr.artifact_location,
        onnxruntime: ztldr.onnxruntime,
        model: ztldr.model,
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

fn apply_global_ztldr_config(config: &mut TldrConfig, global: GlobalZtldrConfig) {
    if let Some(enabled) = global.enabled {
        config.ztldr.enabled = enabled;
    }
    if let Some(artifact_location) = global.artifact_location {
        config.ztldr.artifact_location = artifact_location;
    }
    if let Some(onnxruntime) = global.onnxruntime {
        config.ztldr.onnxruntime = onnxruntime;
    }
    if let Some(model) = global.model {
        config.semantic.model = model;
    }
}

fn apply_global_ztldr_runtime_config(config: &mut TldrConfig) {
    if !config.ztldr.uses_onnxruntime() {
        config.semantic.embedding.enabled = false;
        config.semantic.embedding_enabled = false;
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

        assert_eq!(
            config.clone().into_runtime_or_default(),
            ZtldrConfig::default()
        );
        assert_eq!(config.model, None);
        assert!(!config.into_runtime_or_default().uses_project_artifacts());
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
            config.clone().into_runtime_or_default(),
            ZtldrConfig {
                enabled: true,
                artifact_location: ZtldrArtifactLocation::Project,
                onnxruntime: true,
            }
        );
        assert_eq!(config.model, None);
        assert!(
            config
                .clone()
                .into_runtime_or_default()
                .uses_project_artifacts()
        );
        assert!(config.into_runtime_or_default().uses_onnxruntime());
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

        assert_eq!(
            config.artifact_location,
            Some(ZtldrArtifactLocation::Project)
        );
        assert_eq!(
            config.clone().into_runtime_or_default().artifact_location,
            ZtldrArtifactLocation::Project
        );
        assert!(!config.into_runtime_or_default().uses_project_artifacts());
    }

    #[test]
    fn load_global_ztldr_config_can_disable_onnxruntime() {
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nonnxruntime = false\n",
        )
        .expect("global config should write");

        let config = load_global_ztldr_config_from_codex_home(Some(codex_home.path()))
            .expect("global ztldr config should load");

        assert!(!config.into_runtime_or_default().uses_onnxruntime());
    }

    #[test]
    fn load_global_ztldr_config_reads_model() {
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nmodel = \"jina-code\"\n",
        )
        .expect("global config should write");

        let config = load_global_ztldr_config_from_codex_home(Some(codex_home.path()))
            .expect("global ztldr config should load");

        assert_eq!(config.model.as_deref(), Some("jina-code"));
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
        assert!(config.ztldr.uses_onnxruntime());
    }

    #[test]
    fn load_tldr_config_applies_model_from_global_config() {
        let project = tempdir().expect("project should exist");
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nmodel = \"jina-code\"\n",
        )
        .expect("global config should write");

        let config = load_tldr_config_with_codex_home(project.path(), Some(codex_home.path()))
            .expect("config should load");

        assert_eq!(config.semantic.model, "jina-code");
    }

    #[test]
    fn load_tldr_config_project_config_model_overrides_global_config_model() {
        let project = tempdir().expect("project should exist");
        let codex_dir = project.path().join(".codex");
        std::fs::create_dir(&codex_dir).expect("project config dir should exist");
        std::fs::write(
            codex_dir.join("config.toml"),
            "[ztldr]\nmodel = \"minilm\"\n",
        )
        .expect("project config should write");
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nmodel = \"jina-code\"\n",
        )
        .expect("global config should write");

        let config = load_tldr_config_with_codex_home(project.path(), Some(codex_home.path()))
            .expect("config should load");

        assert_eq!(config.semantic.model, "minilm");
    }

    #[test]
    fn load_tldr_config_tldr_toml_model_overrides_config_toml_model() {
        let project = tempdir().expect("project should exist");
        let codex_dir = project.path().join(".codex");
        std::fs::create_dir(&codex_dir).expect("project config dir should exist");
        std::fs::write(
            codex_dir.join("config.toml"),
            "[ztldr]\nmodel = \"minilm\"\n",
        )
        .expect("project config should write");
        std::fs::write(
            codex_dir.join("tldr.toml"),
            "[semantic]\nmodel = \"jina-code\"\n",
        )
        .expect("project tldr config should write");

        let config =
            load_tldr_config_with_codex_home(project.path(), None).expect("config should load");

        assert_eq!(config.semantic.model, "jina-code");
    }

    #[test]
    fn load_tldr_config_global_onnxruntime_false_disables_embedding_backend() {
        let project = tempdir().expect("project should exist");
        let codex_dir = project.path().join(".codex");
        std::fs::create_dir(&codex_dir).expect("project config dir should exist");
        std::fs::write(
            codex_dir.join("tldr.toml"),
            "[semantic.embedding]\nenabled = true\ndimensions = 128\n",
        )
        .expect("project config should write");
        let codex_home = tempdir().expect("codex home should exist");
        std::fs::write(
            codex_home.path().join("config.toml"),
            "[ztldr]\nonnxruntime = false\n",
        )
        .expect("global config should write");

        let config = load_tldr_config_with_codex_home(project.path(), Some(codex_home.path()))
            .expect("config should load");

        assert!(!config.ztldr.uses_onnxruntime());
        assert!(!config.semantic.embedding.enabled);
        assert!(!config.semantic.embedding_enabled);
        assert_eq!(config.semantic.embedding.dimensions, 128);
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

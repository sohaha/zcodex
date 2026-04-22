use std::fs;
use std::path::Path;
use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use thiserror::Error;

use crate::EnvelopeId;
use crate::InstanceId;

pub const STATE_VERSION: u32 = 1;
pub const MANIFEST_FILENAME: &str = "manifest.json";
pub const DAEMON_DIRNAME: &str = "daemon";
pub const DAEMON_ENDPOINT_FILENAME: &str = "endpoint";
pub const DAEMON_PID_FILENAME: &str = "pid";
pub const INSTANCES_DIRNAME: &str = "instances";
pub const CARDS_DIRNAME: &str = "cards";
pub const LEASES_DIRNAME: &str = "leases";
pub const HEARTBEATS_DIRNAME: &str = "heartbeats";
pub const MAILBOXES_DIRNAME: &str = "mailboxes";
pub const INBOX_DIRNAME: &str = "inbox";
pub const ACKS_DIRNAME: &str = "acks";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FederationStateManifest {
    pub version: u32,
    pub created_at: i64,
}

impl FederationStateManifest {
    pub const fn new(created_at: i64) -> Self {
        Self {
            version: STATE_VERSION,
            created_at,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.version != STATE_VERSION {
            return Err(format!(
                "unsupported federation state version: expected {STATE_VERSION}, got {}",
                self.version
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FederationStateLayout {
    root: PathBuf,
}

impl FederationStateLayout {
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StateLayoutError> {
        let root = root.into();
        if !root.is_absolute() {
            return Err(StateLayoutError::RootMustBeAbsolute(root));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.root.join(MANIFEST_FILENAME)
    }

    pub fn daemon_dir(&self) -> PathBuf {
        self.root.join(DAEMON_DIRNAME)
    }

    pub fn daemon_endpoint_path(&self) -> PathBuf {
        self.daemon_dir().join(DAEMON_ENDPOINT_FILENAME)
    }

    pub fn daemon_pid_path(&self) -> PathBuf {
        self.daemon_dir().join(DAEMON_PID_FILENAME)
    }

    pub fn instance_card_path(&self, instance_id: InstanceId) -> PathBuf {
        self.root
            .join(INSTANCES_DIRNAME)
            .join(CARDS_DIRNAME)
            .join(json_file_name(instance_id))
    }

    pub fn lease_path(&self, instance_id: InstanceId) -> PathBuf {
        self.root
            .join(INSTANCES_DIRNAME)
            .join(LEASES_DIRNAME)
            .join(json_file_name(instance_id))
    }

    pub fn heartbeat_path(&self, instance_id: InstanceId) -> PathBuf {
        self.root
            .join(INSTANCES_DIRNAME)
            .join(HEARTBEATS_DIRNAME)
            .join(json_file_name(instance_id))
    }

    pub fn mailbox_dir(&self, recipient: InstanceId) -> PathBuf {
        self.root
            .join(MAILBOXES_DIRNAME)
            .join(recipient.to_string())
    }

    pub fn inbox_dir(&self, recipient: InstanceId) -> PathBuf {
        self.mailbox_dir(recipient).join(INBOX_DIRNAME)
    }

    pub fn envelope_path(&self, recipient: InstanceId, envelope_id: EnvelopeId) -> PathBuf {
        self.inbox_dir(recipient).join(json_file_name(envelope_id))
    }

    pub fn ack_path(&self, envelope_id: EnvelopeId) -> PathBuf {
        self.root
            .join(ACKS_DIRNAME)
            .join(json_file_name(envelope_id))
    }

    pub fn temp_path(&self, stable_path: &Path) -> Result<PathBuf, StateLayoutError> {
        let stable_file_name = self.ensure_managed_json_path(stable_path)?;
        Ok(stable_path.with_file_name(format!("{stable_file_name}.tmp")))
    }

    pub fn write_json<T: Serialize>(
        &self,
        stable_path: &Path,
        value: &T,
    ) -> Result<(), StateIoError> {
        let temp_path = self.temp_path(stable_path)?;
        let Some(parent) = stable_path.parent() else {
            return Err(
                StateLayoutError::ManagedPathMustHaveParent(stable_path.to_path_buf()).into(),
            );
        };

        fs::create_dir_all(parent)?;
        fs::write(&temp_path, serde_json::to_vec_pretty(value)?)?;
        if stable_path.exists() {
            fs::remove_file(stable_path)?;
        }
        fs::rename(temp_path, stable_path)?;
        Ok(())
    }

    pub fn read_json<T: DeserializeOwned>(&self, stable_path: &Path) -> Result<T, StateIoError> {
        self.ensure_managed_json_path(stable_path)?;
        let bytes = fs::read(stable_path)?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    fn ensure_managed_json_path(&self, stable_path: &Path) -> Result<String, StateLayoutError> {
        let relative_path = stable_path.strip_prefix(&self.root).map_err(|_| {
            StateLayoutError::ManagedPathOutsideRoot {
                root: self.root.clone(),
                path: stable_path.to_path_buf(),
            }
        })?;
        if relative_path.as_os_str().is_empty() {
            return Err(StateLayoutError::ManagedPathMustBeFile(
                stable_path.to_path_buf(),
            ));
        }
        let file_name = stable_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| StateLayoutError::ManagedPathMustBeUtf8(stable_path.to_path_buf()))?;
        if file_name.ends_with(".tmp") || !file_name.ends_with(".json") {
            return Err(StateLayoutError::ManagedPathMustBeStableJson(
                stable_path.to_path_buf(),
            ));
        }
        Ok(file_name.to_string())
    }
}

#[derive(Debug, Error)]
pub enum StateLayoutError {
    #[error("federation state root must be absolute: {0}")]
    RootMustBeAbsolute(PathBuf),
    #[error("managed federation state path must stay under {root}: {path}")]
    ManagedPathOutsideRoot { root: PathBuf, path: PathBuf },
    #[error("managed federation state path must be a file path: {0}")]
    ManagedPathMustBeFile(PathBuf),
    #[error("managed federation state path must be valid utf-8: {0}")]
    ManagedPathMustBeUtf8(PathBuf),
    #[error("managed federation state path must be a stable .json file: {0}")]
    ManagedPathMustBeStableJson(PathBuf),
    #[error("managed federation state path must have a parent directory: {0}")]
    ManagedPathMustHaveParent(PathBuf),
}

#[derive(Debug, Error)]
pub enum StateIoError {
    #[error(transparent)]
    Layout(#[from] StateLayoutError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

fn json_file_name(id: impl std::fmt::Display) -> String {
    format!("{id}.json")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use pretty_assertions::assert_eq;
    use tempfile::TempDir;

    use crate::DAEMON_DIRNAME;
    use crate::DAEMON_ENDPOINT_FILENAME;
    use crate::DAEMON_PID_FILENAME;
    use crate::Envelope;
    use crate::EnvelopeId;
    use crate::EnvelopePayload;
    use crate::FederationStateLayout;
    use crate::FederationStateManifest;
    use crate::InstanceId;

    #[test]
    fn computes_expected_paths() {
        let tempdir = TempDir::new().expect("tempdir");
        let layout = FederationStateLayout::new(tempdir.path()).expect("layout");
        let instance_id = InstanceId::default();
        let envelope_id = EnvelopeId::default();

        assert_eq!(
            layout.daemon_endpoint_path(),
            tempdir
                .path()
                .join(DAEMON_DIRNAME)
                .join(DAEMON_ENDPOINT_FILENAME)
        );
        assert_eq!(
            layout.daemon_pid_path(),
            tempdir
                .path()
                .join(DAEMON_DIRNAME)
                .join(DAEMON_PID_FILENAME)
        );
        assert_eq!(
            layout.instance_card_path(instance_id),
            tempdir
                .path()
                .join("instances")
                .join("cards")
                .join(format!("{instance_id}.json"))
        );
        assert_eq!(
            layout.envelope_path(instance_id, envelope_id),
            tempdir
                .path()
                .join("mailboxes")
                .join(instance_id.to_string())
                .join("inbox")
                .join(format!("{envelope_id}.json"))
        );
        assert_eq!(
            layout.ack_path(envelope_id),
            tempdir
                .path()
                .join("acks")
                .join(format!("{envelope_id}.json"))
        );
    }

    #[test]
    fn round_trips_json_through_stable_file_paths() {
        let tempdir = TempDir::new().expect("tempdir");
        let layout = FederationStateLayout::new(tempdir.path()).expect("layout");
        let recipient = InstanceId::default();
        let envelope = Envelope {
            envelope_id: EnvelopeId::default(),
            sender: InstanceId::default(),
            recipient,
            created_at: 1_710_000_000,
            expires_at: 1_710_000_060,
            payload: EnvelopePayload::TextTask {
                text: "run cargo test".to_string(),
            },
        };
        let path = layout.envelope_path(recipient, envelope.envelope_id);

        layout.write_json(&path, &envelope).expect("write");
        let loaded: Envelope = layout.read_json(&path).expect("read");

        assert_eq!(loaded, envelope);
        assert!(!layout.temp_path(&path).expect("temp path").exists());
    }

    #[test]
    fn rejects_paths_outside_root_or_tmp_suffixes() {
        let tempdir = TempDir::new().expect("tempdir");
        let layout = FederationStateLayout::new(tempdir.path()).expect("layout");
        let outside = Path::new("/tmp/outside.json");
        let tmp_like = tempdir.path().join("acks").join("item.json.tmp");

        assert_eq!(
            layout
                .temp_path(outside)
                .expect_err("outside root")
                .to_string(),
            format!(
                "managed federation state path must stay under {}: {}",
                tempdir.path().display(),
                outside.display()
            )
        );
        assert_eq!(
            layout
                .temp_path(&tmp_like)
                .expect_err("tmp suffix")
                .to_string(),
            format!(
                "managed federation state path must be a stable .json file: {}",
                tmp_like.display()
            )
        );
    }

    #[test]
    fn manifest_version_is_fixed() {
        let manifest = FederationStateManifest::new(1_710_000_000);
        assert_eq!(manifest.validate(), Ok(()));
        assert_eq!(manifest.version, 1);
    }
}

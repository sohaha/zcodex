//! Federation protocol and local state layout for multi-instance Codex peers.

mod daemon_api;
mod envelope;
mod envelope_id;
mod instance_card;
mod instance_id;
mod lease;
mod state_layout;

pub use daemon_api::FederationCleanupReport;
pub use daemon_api::FederationDaemonCommand;
pub use daemon_api::FederationDaemonResponse;
pub use envelope::AckState;
pub use envelope::Envelope;
pub use envelope::EnvelopeAck;
pub use envelope::EnvelopePayload;
pub use envelope_id::EnvelopeId;
pub use instance_card::InstanceCard;
pub use instance_id::InstanceId;
pub use lease::Heartbeat;
pub use lease::Lease;
pub use state_layout::ACKS_DIRNAME;
pub use state_layout::CARDS_DIRNAME;
pub use state_layout::DAEMON_DIRNAME;
pub use state_layout::DAEMON_ENDPOINT_FILENAME;
pub use state_layout::DAEMON_PID_FILENAME;
pub use state_layout::FederationStateLayout;
pub use state_layout::FederationStateManifest;
pub use state_layout::HEARTBEATS_DIRNAME;
pub use state_layout::INBOX_DIRNAME;
pub use state_layout::INSTANCES_DIRNAME;
pub use state_layout::LEASES_DIRNAME;
pub use state_layout::MAILBOXES_DIRNAME;
pub use state_layout::MANIFEST_FILENAME;
pub use state_layout::STATE_VERSION;
pub use state_layout::StateIoError;
pub use state_layout::StateLayoutError;

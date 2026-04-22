use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::Envelope;
use crate::EnvelopeAck;
use crate::Heartbeat;
use crate::InstanceCard;
use crate::InstanceId;
use crate::Lease;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum FederationDaemonCommand {
    Ping,
    RegisterInstance {
        card: InstanceCard,
    },
    Heartbeat {
        instance_id: InstanceId,
        lease: Lease,
        heartbeat: Heartbeat,
    },
    ListPeers {
        requester: Option<InstanceId>,
        now: i64,
    },
    SendEnvelope {
        envelope: Envelope,
    },
    ReadInbox {
        recipient: InstanceId,
        now: i64,
    },
    WriteAck {
        ack: EnvelopeAck,
    },
    Cleanup {
        now: i64,
    },
    Shutdown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct FederationCleanupReport {
    pub expired_instances_removed: u64,
    pub expired_envelopes_removed: u64,
    pub acks_updated: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FederationDaemonResponse {
    pub ok: bool,
    pub message: String,
    pub card: Option<InstanceCard>,
    pub peers: Option<Vec<InstanceCard>>,
    pub envelopes: Option<Vec<Envelope>>,
    pub ack: Option<EnvelopeAck>,
    pub cleanup: Option<FederationCleanupReport>,
}

impl FederationDaemonResponse {
    pub fn ok(message: impl Into<String>) -> Self {
        Self {
            ok: true,
            message: message.into(),
            card: None,
            peers: None,
            envelopes: None,
            ack: None,
            cleanup: None,
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self {
            ok: false,
            message: message.into(),
            card: None,
            peers: None,
            envelopes: None,
            ack: None,
            cleanup: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::FederationDaemonCommand;

    #[test]
    fn command_tags_use_stable_snake_case() {
        let value = serde_json::to_value(FederationDaemonCommand::Ping).expect("json");
        assert_eq!(value["command"], "ping");
    }
}

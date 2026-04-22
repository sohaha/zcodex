use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::EnvelopeId;
use crate::InstanceId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AckState {
    Accepted,
    Delivered,
    Expired,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct EnvelopeAck {
    pub envelope_id: EnvelopeId,
    pub recipient: InstanceId,
    pub state: AckState,
    pub updated_at: i64,
    pub detail: Option<String>,
}

impl EnvelopeAck {
    pub fn validate(&self) -> Result<(), String> {
        if let Some(detail) = &self.detail {
            validate_text_field("detail", detail)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Envelope {
    pub envelope_id: EnvelopeId,
    pub sender: InstanceId,
    pub recipient: InstanceId,
    pub created_at: i64,
    pub expires_at: i64,
    pub payload: EnvelopePayload,
}

impl Envelope {
    pub fn validate(&self) -> Result<(), String> {
        if self.expires_at <= self.created_at {
            return Err(format!(
                "envelope expires_at must be greater than created_at: {} <= {}",
                self.expires_at, self.created_at
            ));
        }
        self.payload.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EnvelopePayload {
    TextTask {
        text: String,
    },
    TextResult {
        in_reply_to: EnvelopeId,
        text: String,
    },
}

impl EnvelopePayload {
    fn validate(&self) -> Result<(), String> {
        match self {
            Self::TextTask { text } | Self::TextResult { text, .. } => {
                validate_text_field("text", text)
            }
        }
    }
}

fn validate_text_field(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} must not be blank"));
    }
    if value.chars().any(char::is_control) {
        return Err(format!("{field} must not contain control characters"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use crate::AckState;
    use crate::Envelope;
    use crate::EnvelopeAck;
    use crate::EnvelopeId;
    use crate::EnvelopePayload;
    use crate::InstanceId;

    #[test]
    fn serializes_text_task_with_stable_tag() {
        let envelope = Envelope {
            envelope_id: EnvelopeId::default(),
            sender: InstanceId::default(),
            recipient: InstanceId::default(),
            created_at: 1_710_000_000,
            expires_at: 1_710_000_060,
            payload: EnvelopePayload::TextTask {
                text: "summarize repo status".to_string(),
            },
        };

        let json = serde_json::to_value(&envelope).expect("envelope json");
        assert_eq!(json["payload"]["kind"], "text_task");
        assert_eq!(envelope.validate(), Ok(()));
    }

    #[test]
    fn rejects_expired_envelope_window() {
        let envelope = Envelope {
            envelope_id: EnvelopeId::default(),
            sender: InstanceId::default(),
            recipient: InstanceId::default(),
            created_at: 12,
            expires_at: 12,
            payload: EnvelopePayload::TextTask {
                text: "ping".to_string(),
            },
        };

        assert_eq!(
            envelope.validate(),
            Err("envelope expires_at must be greater than created_at: 12 <= 12".to_string())
        );
    }

    #[test]
    fn ack_detail_must_not_be_blank() {
        let ack = EnvelopeAck {
            envelope_id: EnvelopeId::default(),
            recipient: InstanceId::default(),
            state: AckState::Accepted,
            updated_at: 99,
            detail: Some("  ".to_string()),
        };

        assert_eq!(ack.validate(), Err("detail must not be blank".to_string()));
    }
}

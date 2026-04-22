use std::fmt::Display;

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::Schema;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EnvelopeId {
    uuid: Uuid,
}

impl EnvelopeId {
    pub fn new() -> Self {
        Self {
            uuid: Uuid::now_v7(),
        }
    }

    pub fn from_string(value: &str) -> Result<Self, uuid::Error> {
        Ok(Self {
            uuid: Uuid::parse_str(value)?,
        })
    }
}

impl Default for EnvelopeId {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<&str> for EnvelopeId {
    type Error = uuid::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_string(value)
    }
}

impl TryFrom<String> for EnvelopeId {
    type Error = uuid::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_string(value.as_str())
    }
}

impl From<EnvelopeId> for String {
    fn from(value: EnvelopeId) -> Self {
        value.to_string()
    }
}

impl Display for EnvelopeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.uuid, f)
    }
}

impl Serialize for EnvelopeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&self.uuid)
    }
}

impl<'de> Deserialize<'de> for EnvelopeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let uuid = Uuid::parse_str(&value).map_err(serde::de::Error::custom)?;
        Ok(Self { uuid })
    }
}

impl JsonSchema for EnvelopeId {
    fn schema_name() -> String {
        "EnvelopeId".to_string()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <String>::json_schema(generator)
    }
}

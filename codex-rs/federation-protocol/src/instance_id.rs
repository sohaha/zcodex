use std::fmt::Display;

use schemars::JsonSchema;
use schemars::r#gen::SchemaGenerator;
use schemars::schema::Schema;
use serde::Deserialize;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstanceId {
    uuid: Uuid,
}

impl InstanceId {
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

impl Default for InstanceId {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<&str> for InstanceId {
    type Error = uuid::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_string(value)
    }
}

impl TryFrom<String> for InstanceId {
    type Error = uuid::Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_string(value.as_str())
    }
}

impl From<InstanceId> for String {
    fn from(value: InstanceId) -> Self {
        value.to_string()
    }
}

impl Display for InstanceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.uuid, f)
    }
}

impl Serialize for InstanceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(&self.uuid)
    }
}

impl<'de> Deserialize<'de> for InstanceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        let uuid = Uuid::parse_str(&value).map_err(serde::de::Error::custom)?;
        Ok(Self { uuid })
    }
}

impl JsonSchema for InstanceId {
    fn schema_name() -> String {
        "InstanceId".to_string()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <String>::json_schema(generator)
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use uuid::Uuid;

    use super::InstanceId;

    #[test]
    fn default_is_not_nil() {
        let id = InstanceId::default();
        let parsed = Uuid::parse_str(&id.to_string()).expect("instance id should be a uuid");
        assert_ne!(parsed, Uuid::nil());
    }

    #[test]
    fn rejects_agent_path_like_values() {
        let err = InstanceId::try_from("/root/researcher").expect_err("agent path must not parse");
        assert_eq!(
            err.to_string(),
            "invalid character: expected an optional prefix of `urn:uuid:` followed by [0-9a-fA-F-], found `/` at 1"
        );
    }
}

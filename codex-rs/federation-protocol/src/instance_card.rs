use std::path::PathBuf;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

use crate::Heartbeat;
use crate::InstanceId;
use crate::Lease;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InstanceCard {
    pub instance_id: InstanceId,
    pub display_name: String,
    pub role: Option<String>,
    pub task_scope: Option<String>,
    pub cwd: PathBuf,
    pub registered_at: i64,
    pub lease: Lease,
    pub heartbeat: Heartbeat,
}

impl InstanceCard {
    pub fn validate(&self) -> Result<(), String> {
        validate_text_field("display_name", &self.display_name)?;
        validate_optional_text_field("role", self.role.as_deref())?;
        validate_optional_text_field("task_scope", self.task_scope.as_deref())?;
        if !self.cwd.is_absolute() {
            return Err(format!(
                "instance cwd must be absolute: {}",
                self.cwd.display()
            ));
        }
        self.lease.validate()?;
        if self.registered_at > self.heartbeat.observed_at {
            return Err(format!(
                "instance heartbeat observed_at must be greater than or equal to registered_at: {} < {}",
                self.heartbeat.observed_at, self.registered_at
            ));
        }
        if self.lease.issued_at < self.registered_at {
            return Err(format!(
                "instance lease issued_at must be greater than or equal to registered_at: {} < {}",
                self.lease.issued_at, self.registered_at
            ));
        }
        if self.heartbeat.observed_at > self.lease.expires_at {
            return Err(format!(
                "instance heartbeat observed_at must not exceed lease expires_at: {} > {}",
                self.heartbeat.observed_at, self.lease.expires_at
            ));
        }
        Ok(())
    }
}

fn validate_optional_text_field(field: &str, value: Option<&str>) -> Result<(), String> {
    if let Some(value) = value {
        validate_text_field(field, value)?;
    }
    Ok(())
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
    use std::path::PathBuf;

    use pretty_assertions::assert_eq;

    use crate::Heartbeat;
    use crate::InstanceCard;
    use crate::InstanceId;
    use crate::Lease;

    #[test]
    fn accepts_display_names_that_are_not_agent_path_segments() {
        let card = InstanceCard {
            instance_id: InstanceId::default(),
            display_name: "planner.alpha-01".to_string(),
            role: Some("planner".to_string()),
            task_scope: Some("repo-a".to_string()),
            cwd: PathBuf::from("/workspace/repo-a"),
            registered_at: 1_710_000_000,
            lease: Lease::new(1_710_000_000, 30).expect("lease"),
            heartbeat: Heartbeat::new(3, 1_710_000_010),
        };

        assert_eq!(card.validate(), Ok(()));
    }

    #[test]
    fn rejects_relative_cwd() {
        let card = InstanceCard {
            instance_id: InstanceId::default(),
            display_name: "planner".to_string(),
            role: None,
            task_scope: None,
            cwd: PathBuf::from("workspace/repo-a"),
            registered_at: 1_710_000_000,
            lease: Lease::new(1_710_000_000, 30).expect("lease"),
            heartbeat: Heartbeat::new(1, 1_710_000_001),
        };

        assert_eq!(
            card.validate(),
            Err("instance cwd must be absolute: workspace/repo-a".to_string())
        );
    }
}

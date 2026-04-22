use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Lease {
    pub issued_at: i64,
    pub ttl_secs: u32,
    pub expires_at: i64,
}

impl Lease {
    pub fn new(issued_at: i64, ttl_secs: u32) -> Result<Self, String> {
        if ttl_secs == 0 {
            return Err("lease ttl_secs must be greater than zero".to_string());
        }
        let expires_at = issued_at
            .checked_add(i64::from(ttl_secs))
            .ok_or_else(|| "lease expires_at overflowed".to_string())?;
        Ok(Self {
            issued_at,
            ttl_secs,
            expires_at,
        })
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.ttl_secs == 0 {
            return Err("lease ttl_secs must be greater than zero".to_string());
        }
        let expected_expires_at = self
            .issued_at
            .checked_add(i64::from(self.ttl_secs))
            .ok_or_else(|| "lease expires_at overflowed".to_string())?;
        if self.expires_at != expected_expires_at {
            return Err(format!(
                "lease expires_at must equal issued_at + ttl_secs: expected {expected_expires_at}, got {}",
                self.expires_at
            ));
        }
        Ok(())
    }

    pub fn is_active_at(&self, unix_seconds: i64) -> bool {
        unix_seconds < self.expires_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Heartbeat {
    pub sequence: u64,
    pub observed_at: i64,
}

impl Heartbeat {
    pub const fn new(sequence: u64, observed_at: i64) -> Self {
        Self {
            sequence,
            observed_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::Lease;

    #[test]
    fn lease_uses_deterministic_expiry() {
        let lease = Lease::new(120, 30).expect("lease");
        assert_eq!(
            lease,
            Lease {
                issued_at: 120,
                ttl_secs: 30,
                expires_at: 150,
            }
        );
        assert!(lease.is_active_at(149));
        assert!(!lease.is_active_at(150));
    }

    #[test]
    fn lease_validation_rejects_mismatched_expiry() {
        let lease = Lease {
            issued_at: 120,
            ttl_secs: 30,
            expires_at: 170,
        };
        assert_eq!(
            lease.validate(),
            Err(
                "lease expires_at must equal issued_at + ttl_secs: expected 150, got 170"
                    .to_string()
            )
        );
    }
}

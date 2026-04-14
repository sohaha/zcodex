//! PostHog analytics client for user fingerprinting and startup events.

use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{debug, warn};

const POSTHOG_API_VERSION: &str = "3";
const POSTHOG_TIMEOUT: Duration = Duration::from_secs(5);

/// PostHog event client.
#[derive(Clone)]
pub struct PostHogClient {
    api_key: String,
    client: reqwest::blocking::Client,
    enabled: bool,
}

impl PostHogClient {
    /// Create a new PostHog client.
    pub fn new(api_key: String) -> Result<Self, reqwest::Error> {
        let client = reqwest::blocking::Client::builder()
            .timeout(POSTHOG_TIMEOUT)
            .build()?;
        
        Ok(Self {
            api_key,
            client,
            enabled: !api_key.is_empty(),
        })
    }

    /// Create a disabled PostHog client (no-op).
    pub fn disabled() -> Self {
        Self {
            api_key: String::new(),
            client: reqwest::blocking::Client::builder()
                .timeout(POSTHOG_TIMEOUT)
                .build()
                .expect("create default reqwest client"),
            enabled: false,
        }
    }

    /// Capture an event.
    pub fn capture(&self, event: PostHogEvent) -> Result<(), PostHogError> {
        if !self.enabled {
            return Ok(());
        }

        let payload = PostHogPayload {
            api_key: self.api_key.clone(),
            event: event.event_name,
            distinct_id: event.distinct_id,
            properties: event.properties,
            timestamp: Some(event.timestamp),
        };

        debug!("capturing PostHog event: {}", payload.event);

        let response = self
            .client
            .post("https://app.posthog.com/capture/" )
            .header("Content-Type", "application/json")
            .json(&payload)
            .send();

        match response {
            Ok(resp) if resp.status().is_success() => {
                debug!("PostHog event captured successfully");
                Ok(())
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().unwrap_or_default();
                warn!("PostHog capture failed: {} - {}", status, body);
                Err(PostHogError::HttpError {
                    status: status.as_u16(),
                    message: body,
                })
            }
            Err(err) => {
                warn!("PostHog capture request failed: {}", err);
                Err(PostHogError::RequestError(err.to_string()))
            }
        }
    }

    /// Alias a distinct ID to another ID (e.g., anonymous to authenticated).
    pub fn alias(&self, distinct_id: &str, alias: &str) -> Result<(), PostHogError> {
        if !self.enabled {
            return Ok(());
        }

        let payload = PostHogAliasPayload {
            api_key: self.api_key.clone(),
            distinct_id: distinct_id.to_string(),
            alias: alias.to_string(),
        };

        debug!("aliasing PostHog ID: {} -> {}", distinct_id, alias);

        let response = self
            .client
            .post("https://app.posthog.com/capture/")
            .header("Content-Type", "application/json")
            .json(&payload)
            .send();

        match response {
            Ok(resp) if resp.status().is_success() => {
                debug!("PostHog alias created successfully");
                Ok(())
            }
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().unwrap_or_default();
                warn!("PostHog alias failed: {} - {}", status, body);
                Err(PostHogError::HttpError {
                    status: status.as_u16(),
                    message: body,
                })
            }
            Err(err) => {
                warn!("PostHog alias request failed: {}", err);
                Err(PostHogError::RequestError(err.to_string()))
            }
        }
    }
}

/// A PostHog event to capture.
pub struct PostHogEvent {
    /// Unique identifier for the user/device.
    pub distinct_id: String,
    /// Event name (e.g., "cli_started", "cli_command_run").
    pub event_name: String,
    /// Event properties.
    pub properties: serde_json::Value,
    /// Event timestamp (Unix milliseconds).
    pub timestamp: u64,
}

impl PostHogEvent {
    /// Create a new event with the current timestamp.
    pub fn new(distinct_id: String, event_name: String) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self {
            distinct_id,
            event_name,
            properties: serde_json::json!({}),
            timestamp,
        }
    }

    /// Add a property to the event.
    pub fn with_property(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        if let Ok(obj) = self.properties.as_object_mut() {
            obj.insert(key.to_string(), value.into());
        }
        self
    }
}

/// PostHog capture payload.
#[derive(Serialize)]
struct PostHogPayload {
    api_key: String,
    event: String,
    distinct_id: String,
    properties: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
}

/// PostHog alias payload.
#[derive(Serialize)]
struct PostHogAliasPayload {
    api_key: String,
    distinct_id: String,
    alias: String,
}

/// PostHog client errors.
#[derive(Debug, thiserror::Error)]
pub enum PostHogError {
    #[error("HTTP error {status}: {message}")]
    HttpError { status: u16, message: String },

    #[error("Request error: {0}")]
    RequestError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_posthog_event_new() {
        let event = PostHogEvent::new("test-id".to_string(), "test_event".to_string());
        assert_eq!(event.distinct_id, "test-id");
        assert_eq!(event.event_name, "test_event");
        assert!(event.timestamp > 0);
    }

    #[test]
    fn test_posthog_event_with_property() {
        let event = PostHogEvent::new("test-id".to_string(), "test_event".to_string())
            .with_property("key", "value")
            .with_property("number", 42);
        
        assert_eq!(event.properties["key"], "value");
        assert_eq!(event.properties["number"], 42);
    }

    #[test]
    fn test_posthog_client_disabled() {
        let client = PostHogClient::disabled();
        let event = PostHogEvent::new("test-id".to_string(), "test_event".to_string());
        
        // Should not error when disabled
        assert!(client.capture(event).is_ok());
    }
}

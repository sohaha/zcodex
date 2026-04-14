//! PostHog analytics client for user fingerprinting and startup events.

use posthog_rs::Event;
use posthog_rs::capture;
use posthog_rs::init_global;
use tracing::debug;
use tracing::warn;

const DEFAULT_POSTHOG_KEY: &str = "phc_tnpmpHSZKvoVp5A62kD7dAtje5fTTMNCafvpizv5BZTe";

/// PostHog event client wrapper.
#[derive(Clone)]
pub struct PostHogClient {
    api_key: String,
    enabled: bool,
}

impl PostHogClient {
    /// Create a new PostHog client with the default API key.
    pub fn new_with_default_key() -> Result<Self, reqwest::Error> {
        Self::new(DEFAULT_POSTHOG_KEY.to_string())
    }

    /// Create a new PostHog client with custom API key.
    pub fn new(api_key: String) -> Result<Self, reqwest::Error> {
        let enabled = !api_key.is_empty();
        Ok(Self { api_key, enabled })
    }

    /// Create a disabled PostHog client (no-op).
    pub fn disabled() -> Self {
        Self {
            api_key: String::new(),
            enabled: false,
        }
    }

    /// Capture an event using the global client with runtime.
    pub fn capture(&self, event: PostHogEvent) -> Result<(), PostHogError> {
        if !self.enabled {
            return Ok(());
        }

        let mut posthog_event = Event::new(&event.event_name, &event.distinct_id);

        // Add properties
        if let Some(obj) = event.properties.as_object() {
            for (key, value) in obj {
                let _ = posthog_event.insert_prop(key, value);
            }
        }

        debug!("capturing PostHog event: {}", event.event_name);

        // Initialize global client and capture event
        let rt = tokio::runtime::Runtime::new().map_err(|e| {
            PostHogError::RuntimeError(format!("Failed to create tokio runtime: {}", e))
        })?;

        rt.block_on(async {
            init_global(self.api_key.as_str()).await?;
            match capture(posthog_event).await {
                Ok(_) => {
                    debug!("PostHog event captured successfully");
                    Ok(())
                }
                Err(e) => {
                    warn!("PostHog capture failed: {}", e);
                    Err(PostHogError::from(e))
                }
            }
        })
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
}

impl PostHogEvent {
    /// Create a new event.
    pub fn new(distinct_id: String, event_name: String) -> Self {
        Self {
            distinct_id,
            event_name,
            properties: serde_json::json!({}),
        }
    }

    /// Add a property to the event.
    pub fn with_property(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        if let Some(obj) = self.properties.as_object_mut() {
            obj.insert(key.to_string(), value.into());
        }
        self
    }
}

/// PostHog client errors.
#[derive(Debug, thiserror::Error)]
pub enum PostHogError {
    #[error("Runtime error: {0}")]
    RuntimeError(String),
    #[error("PostHog error: {0}")]
    PostHogError(String),
}

impl From<posthog_rs::Error> for PostHogError {
    fn from(err: posthog_rs::Error) -> Self {
        PostHogError::PostHogError(format!("{}", err))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_posthog_event_new() {
        let event = PostHogEvent::new("test-id".to_string(), "test_event".to_string());
        assert_eq!(event.distinct_id, "test-id");
        assert_eq!(event.event_name, "test_event");
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

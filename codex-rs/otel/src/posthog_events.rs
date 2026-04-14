//! PostHog event helpers for common Codex CLI events.

use crate::posthog::{PostHogClient, PostHogEvent};

/// Capture CLI startup event.
pub fn capture_cli_startup(
    client: &PostHogClient,
    installation_id: &str,
    version: &str,
    os: &str,
    os_version: &str,
) {
    let event = PostHogEvent::new(installation_id.to_string(), "cli_started".to_string())
        .with_property("version", version)
        .with_property("os", os)
        .with_property("os_version", os_version)
        .with_property("app", "codex")
        .with_property("app_name", "cli");

    if let Err(err) = client.capture(event) {
        tracing::warn!("Failed to capture CLI startup event: {}", err);
    }
}

/// Get OS information for events.
pub fn get_os_info() -> (String, String) {
    let os_info = os_info::get();
    let os_type = os_info.os_type().to_string();
    let os_version = os_info.version().to_string();
    (os_type, os_version)
}

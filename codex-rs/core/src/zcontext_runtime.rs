use std::sync::Arc;

use codex_context_hooks::ZmemoryContext;
use codex_context_hooks::build_session_snapshot;
use codex_features::Feature;
use tracing::warn;

use crate::config::Config;
use crate::session::session::Session;
use crate::session::turn_context::TurnContext;

pub(crate) async fn build_zcontext_snapshot(
    sess: &Arc<Session>,
    turn_context: &TurnContext,
) -> Option<String> {
    if !sess.enabled(Feature::ZContext) {
        return None;
    }

    let config = sess.get_config().await;
    if !config.context_hooks.enabled {
        return None;
    }

    let context = ZmemoryContext::new(
        config.codex_home.as_path().to_path_buf(),
        turn_context.cwd.as_path().to_path_buf(),
        config.zmemory.path.clone(),
        config.zmemory.to_runtime_settings(),
    );

    match build_session_snapshot(
        &context,
        &sess.conversation_id.to_string(),
        &config.context_hooks.to_context_hooks_settings(),
    ) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            warn!("failed to build zcontext snapshot: {err}");
            None
        }
    }
}

use crate::resume_picker::SessionSelection;
use crate::resume_picker::SessionTarget;
use codex_core::CTF_CLEAN_DEFAULT_REPLACEMENT;
use codex_core::CtfCleanOptions;
use codex_core::clean_ctf_rollout;
use codex_core::config::Config;
use codex_core::find_thread_path_by_id_str;
use codex_core::read_session_meta_line;
use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use color_eyre::eyre::eyre;
use std::path::PathBuf;

const CTF_SESSION_MARKER_SNIPPET: &str = "marker=codex-ctf";

pub(crate) async fn clean_resume_selection_if_needed(
    config: &Config,
    session_selection: &SessionSelection,
) -> Result<()> {
    let SessionSelection::Resume(target_session) = session_selection else {
        return Ok(());
    };

    let rollout_path = resolve_rollout_path(config, target_session).await?;
    let session_meta = read_session_meta_line(rollout_path.as_path())
        .await
        .wrap_err_with(|| {
            format!(
                "failed to read session metadata from {}",
                rollout_path.display()
            )
        })?;
    let is_ctf_session = session_meta
        .meta
        .base_instructions
        .as_ref()
        .is_some_and(|instructions| instructions.text.contains(CTF_SESSION_MARKER_SNIPPET));
    if !is_ctf_session {
        return Ok(());
    }

    clean_ctf_rollout(
        rollout_path.as_path(),
        &CtfCleanOptions {
            replacement: CTF_CLEAN_DEFAULT_REPLACEMENT.to_string(),
            dry_run: false,
            create_backup: true,
        },
    )
    .await
    .map_err(|err| {
        eyre!(
            "failed to clean CTF rollout {}: {err}",
            rollout_path.display()
        )
    })?;
    Ok(())
}

async fn resolve_rollout_path(config: &Config, target_session: &SessionTarget) -> Result<PathBuf> {
    if let Some(path) = target_session.path.clone() {
        return Ok(path);
    }

    find_thread_path_by_id_str(
        config.codex_home.as_path(),
        &target_session.thread_id.to_string(),
    )
    .await?
    .ok_or_else(|| {
        eyre!(
            "failed to resolve rollout path for {}",
            target_session.thread_id
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_core::config::ConfigBuilder;
    use codex_protocol::ThreadId;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use uuid::Uuid;

    async fn test_config(temp_home: &TempDir) -> Config {
        ConfigBuilder::default()
            .codex_home(temp_home.path().to_path_buf())
            .build()
            .await
            .expect("load config")
    }

    fn write_rollout(path: &std::path::Path, is_ctf: bool) {
        let base_instructions = if is_ctf {
            "<!-- codex-ctf marker=codex-ctf template=web -->\nCTF mode is enabled."
        } else {
            "Standard session."
        };
        let content = [
            serde_json::json!({
                "timestamp": "2026-04-09T00:00:00Z",
                "type": "session_meta",
                "payload": {
                    "id": Uuid::from_u128(1).to_string(),
                    "timestamp": "2026-04-09T00:00:00Z",
                    "cwd": ".",
                    "originator": "cli",
                    "cli_version": "0.0.0",
                    "source": "cli",
                    "model_provider": "openai",
                    "base_instructions": {
                        "text": base_instructions,
                    }
                }
            }),
            serde_json::json!({
                "timestamp": "2026-04-09T00:00:01Z",
                "type": "response_item",
                "payload": {
                    "type": "message",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "Sorry, but I can't help with that exploit."}]
                }
            }),
            serde_json::json!({
                "timestamp": "2026-04-09T00:00:02Z",
                "type": "response_item",
                "payload": {
                    "type": "reasoning",
                    "summary": [{"type": "summary_text", "text": "refusal summary"}]
                }
            }),
            serde_json::json!({
                "timestamp": "2026-04-09T00:00:03Z",
                "type": "event_msg",
                "payload": {
                    "type": "agent_message",
                    "message": "Sorry, but I can't help with that exploit."
                }
            }),
            serde_json::json!({
                "timestamp": "2026-04-09T00:00:04Z",
                "type": "event_msg",
                "payload": {
                    "type": "agent_reasoning",
                    "text": "hidden refusal reasoning"
                }
            }),
        ]
        .into_iter()
        .map(|line| line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
        std::fs::write(path, format!("{content}\n")).expect("write rollout");
    }

    fn thread_id() -> ThreadId {
        ThreadId::from_string(&Uuid::from_u128(1).to_string()).expect("thread id")
    }

    #[tokio::test]
    async fn clean_resume_selection_if_needed_cleans_ctf_session() {
        let temp_home = TempDir::new().expect("temp home");
        let config = test_config(&temp_home).await;
        let rollout_path = temp_home.path().join("rollout.jsonl");
        write_rollout(&rollout_path, /*is_ctf*/ true);

        clean_resume_selection_if_needed(
            &config,
            &SessionSelection::Resume(SessionTarget {
                path: Some(rollout_path.clone()),
                thread_id: thread_id(),
            }),
        )
        .await
        .expect("clean ctf resume target");

        let cleaned = std::fs::read_to_string(&rollout_path).expect("read cleaned rollout");
        assert!(cleaned.contains(CTF_CLEAN_DEFAULT_REPLACEMENT));
        assert!(!cleaned.contains("\"type\":\"reasoning\""));
        assert!(!cleaned.contains("\"type\":\"agent_reasoning\""));
        let backups = std::fs::read_dir(temp_home.path())
            .expect("list backups")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bak."))
            .count();
        assert_eq!(backups, 1);
    }

    #[tokio::test]
    async fn clean_resume_selection_if_needed_skips_non_ctf_session() {
        let temp_home = TempDir::new().expect("temp home");
        let config = test_config(&temp_home).await;
        let rollout_path = temp_home.path().join("rollout.jsonl");
        write_rollout(&rollout_path, /*is_ctf*/ false);
        let original = std::fs::read_to_string(&rollout_path).expect("read original rollout");

        clean_resume_selection_if_needed(
            &config,
            &SessionSelection::Resume(SessionTarget {
                path: Some(rollout_path.clone()),
                thread_id: thread_id(),
            }),
        )
        .await
        .expect("skip non-ctf resume target");

        let current = std::fs::read_to_string(&rollout_path).expect("read rollout");
        assert_eq!(current, original);
    }
}

use super::*;
use crate::codex::make_session_and_context;
use crate::protocol::SessionSource;
use crate::protocol::SubAgentSource;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use serde_json::json;
use std::fs;
use std::sync::Arc;

#[test]
fn parse_csv_supports_quotes_and_commas() {
    let input = "id,name\n1,\"alpha, beta\"\n2,gamma\n";
    let (headers, rows) = parse_csv(input).expect("csv parse");
    assert_eq!(headers, vec!["id".to_string(), "name".to_string()]);
    assert_eq!(
        rows,
        vec![
            vec!["1".to_string(), "alpha, beta".to_string()],
            vec!["2".to_string(), "gamma".to_string()]
        ]
    );
}

#[test]
fn csv_escape_quotes_when_needed() {
    assert_eq!(csv_escape("simple"), "simple");
    assert_eq!(csv_escape("a,b"), "\"a,b\"");
    assert_eq!(csv_escape("a\"b"), "\"a\"\"b\"");
}

#[test]
fn render_instruction_template_expands_placeholders_and_escapes_braces() {
    let row = json!({
        "path": "src/lib.rs",
        "area": "test",
        "file path": "docs/readme.md",
    });
    let rendered = render_instruction_template(
        "Review {path} in {area}. Also see {file path}. Use {{literal}}.",
        &row,
    );
    assert_eq!(
        rendered,
        "Review src/lib.rs in test. Also see docs/readme.md. Use {literal}."
    );
}

#[test]
fn render_instruction_template_leaves_unknown_placeholders() {
    let row = json!({
        "path": "src/lib.rs",
    });
    let rendered = render_instruction_template("Check {path} then {missing}", &row);
    assert_eq!(rendered, "Check src/lib.rs then {missing}");
}

#[test]
fn ensure_unique_headers_rejects_duplicates() {
    let headers = vec!["path".to_string(), "path".to_string()];
    let Err(err) = ensure_unique_headers(headers.as_slice()) else {
        panic!("expected duplicate header error");
    };
    assert_eq!(
        err,
        FunctionCallError::RespondToModel("csv header path is duplicated".to_string())
    );
}

#[tokio::test]
async fn build_runner_options_uses_project_agent_limits_after_turn_cwd_override() {
    let (session, mut turn) = make_session_and_context().await;
    let workspace = tempfile::tempdir().expect("workspace temp dir");
    let nested = workspace.path().join("nested");
    let dot_codex = workspace.path().join(".codex");
    fs::write(workspace.path().join(".git"), "gitdir: here").expect("seed git marker");
    fs::create_dir_all(&nested).expect("create nested dir");
    fs::create_dir_all(&dot_codex).expect("create .codex dir");
    fs::write(
        dot_codex.join("config.toml"),
        "[agents]\nmax_depth = 4\nmax_threads = 3\n",
    )
    .expect("write project config");
    fs::create_dir_all(turn.config.codex_home.as_path()).expect("create codex home");
    fs::write(
        turn.config.codex_home.join("config.toml"),
        format!(
            "[projects.\"{}\"]\ntrust_level = \"trusted\"\n",
            workspace.path().display()
        ),
    )
    .expect("write home config");
    turn.cwd = AbsolutePathBuf::try_from(nested).expect("nested path should be absolute");
    turn.session_source = SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
        parent_thread_id: session.conversation_id,
        depth: 1,
        parent_model: None,
        agent_path: None,
        agent_nickname: None,
        agent_role: None,
    });

    let options = build_runner_options(&Arc::new(session), &Arc::new(turn), Some(8))
        .await
        .expect("runner options");

    assert_eq!(options.max_concurrency, 3);
    assert_eq!(options.spawn_config.agent_max_depth, 4);
    assert_eq!(options.spawn_config.agent_max_threads, Some(3));
}

use std::path::Path;

use anyhow::Result;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

fn language_fixture(language: &str) -> (&'static str, &'static str) {
    match language {
        "rust" => ("rs", "fn helper() {}\nfn main() { helper(); }\n"),
        "typescript" => (
            "ts",
            "function helper() {}\nfunction main() { helper(); }\n",
        ),
        "javascript" => (
            "js",
            "function helper() {}\nfunction main() { helper(); }\n",
        ),
        other => panic!("unexpected language fixture request: {other}"),
    }
}

#[tokio::test]
async fn tldr_help_localizes_nested_help_subcommand() -> Result<()> {
    let codex_home = TempDir::new()?;
    let output = codex_command(codex_home.path())?
        .args(["ztldr", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let help = String::from_utf8([output.stdout, output.stderr].concat())?;

    assert!(help.contains("显示此消息或指定子命令的帮助"));
    assert!(!help.contains("Print this message or the help of the given subcommand(s)"));

    Ok(())
}

#[tokio::test]
async fn tldr_structure_help_exposes_language_and_lang_alias() -> Result<()> {
    let codex_home = TempDir::new()?;
    let output = codex_command(codex_home.path())?
        .args(["ztldr", "structure", "--help"])
        .assert()
        .success()
        .get_output()
        .clone();
    let help = String::from_utf8([output.stdout, output.stderr].concat())?;

    assert!(help.contains("--language <LANG>"));
    assert!(help.contains("[别名： --lang]"));

    Ok(())
}

#[tokio::test]
async fn tldr_structure_accepts_language_long_flag() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "structure",
            "--language",
            "rust",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["analysis"]["kind"], "ast");
    assert_eq!(payload["action"], "structure");

    Ok(())
}

#[tokio::test]
async fn tldr_structure_json_preserves_graph_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "use crate::auth::Session;\n\nfn helper(session: Session) {}\nfn main() { helper(todo!()); }\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "structure",
            "--lang",
            "rust",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    let details = &payload["analysis"]["details"];
    let nodes = details["nodes"]
        .as_array()
        .expect("nodes should be an array");
    let edges = details["edges"]
        .as_array()
        .expect("edges should be an array");

    assert_eq!(payload["analysis"]["kind"], "ast");
    assert_eq!(payload["action"], "structure");
    assert!(
        nodes
            .iter()
            .any(|node| { node["id"] == "src/lib.rs" && node["kind"] == "file" })
    );
    assert!(
        nodes
            .iter()
            .any(|node| { node["id"] == "helper" && node["kind"] == "function" })
    );
    assert!(
        nodes
            .iter()
            .any(|node| { node["kind"] == "import" && node["id"] == "use crate::auth::Session;" })
    );
    assert!(
        nodes
            .iter()
            .any(|node| { node["kind"] == "reference" && node["id"] == "Session" })
    );

    assert!(edges.iter().any(|edge| {
        edge["kind"] == "contains" && edge["from"] == "src/lib.rs" && edge["to"] == "helper"
    }));
    assert!(edges.iter().any(|edge| {
        edge["kind"] == "calls" && edge["from"] == "main" && edge["to"] == "helper"
    }));
    assert!(
        edges
            .iter()
            .any(|edge| { edge["kind"] == "imports" && edge["to"] == "use crate::auth::Session;" })
    );
    assert!(
        edges
            .iter()
            .any(|edge| { edge["kind"] == "references" && edge["to"] == "Session" })
    );

    assert_eq!(details["symbol_index"][0]["symbol"], "helper");

    Ok(())
}

#[tokio::test]
async fn tldr_context_json_preserves_call_graph_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "context",
            "--lang",
            "rust",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    let details = &payload["analysis"]["details"];
    let edges = details["edges"]
        .as_array()
        .expect("edges should be an array");
    let calls_main_helper = edges
        .iter()
        .filter(|edge| edge["kind"] == "calls" && edge["from"] == "main" && edge["to"] == "helper")
        .count();

    assert_eq!(payload["analysis"]["kind"], "call_graph");
    assert_eq!(payload["action"], "context");
    assert_eq!(calls_main_helper, 1);
    assert_eq!(details["overview"]["incoming_edges"], 1);
    assert!(details["nodes"].as_array().is_some_and(|nodes| {
        nodes
            .iter()
            .any(|node| node["id"] == "helper" && node["kind"] == "function")
    }));

    Ok(())
}

#[tokio::test]
async fn tldr_structure_json_supports_language_matrix() -> Result<()> {
    for language in ["rust", "typescript", "javascript"] {
        let codex_home = TempDir::new()?;
        let project = TempDir::new()?;
        std::fs::create_dir_all(project.path().join("src"))?;
        let (extension, contents) = language_fixture(language);
        std::fs::write(
            project.path().join(format!("src/lib.{extension}")),
            contents,
        )?;

        let mut cmd = codex_command(codex_home.path())?;
        let output = cmd
            .args([
                "ztldr",
                "structure",
                "--lang",
                language,
                "--project",
                project
                    .path()
                    .to_str()
                    .expect("project path should be utf-8"),
                "--json",
            ])
            .assert()
            .success()
            .get_output()
            .stdout
            .clone();

        let payload: serde_json::Value = serde_json::from_slice(&output)?;
        let details = &payload["analysis"]["details"];
        let helper_node = details["nodes"]
            .as_array()
            .expect("nodes should be an array")
            .iter()
            .find(|node| node["id"] == "helper")
            .expect("helper node should exist");

        assert_eq!(payload["analysis"]["kind"], "ast");
        assert_eq!(payload["action"], "structure");
        assert_eq!(helper_node["kind"], "function");
        assert_eq!(details["symbol_index"][0]["symbol"], "helper");
    }

    Ok(())
}

#[tokio::test]
async fn tldr_impact_json_preserves_pdg_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "impact",
            "--lang",
            "rust",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "helper",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    let details = &payload["analysis"]["details"];
    let edges = details["edges"]
        .as_array()
        .expect("edges should be an array");
    let caller_node = details["nodes"]
        .as_array()
        .expect("nodes should be an array")
        .iter()
        .find(|node| node["id"] == "main")
        .expect("main node should exist");
    let calls_main_helper = edges
        .iter()
        .filter(|edge| edge["kind"] == "calls" && edge["from"] == "main" && edge["to"] == "helper")
        .count();

    assert_eq!(payload["analysis"]["kind"], "impact");
    assert_eq!(payload["action"], "impact");
    assert_eq!(details["symbol_query"], "helper");
    assert_eq!(details["overview"]["outgoing_edges"], 1);
    assert_eq!(caller_node["kind"], "function");
    assert_eq!(
        details["units"][0]["dfg_summary"],
        "params=0, locals=0, mutable bindings=0, assignments=0, references=2"
    );
    assert_eq!(calls_main_helper, 1);

    Ok(())
}

#[tokio::test]
async fn tldr_impact_text_renders_summary_lines() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "fn helper() {}\nfn main() { helper(); }\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "impact",
            "--lang",
            "rust",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "helper",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let text = String::from_utf8(output).expect("output should be utf-8");
    assert!(text.contains("语言：rust"));
    assert!(text.contains("来源："));
    assert!(text.contains("支持级别：DataFlow"));
    assert!(text.contains("回退策略：structure + search"));
    assert!(text.contains("消息："));
    assert!(text.contains("摘要：impact summary:"));
    assert!(text.contains("impact summary: 1 callers found for helper across 1 files"));

    Ok(())
}

#[tokio::test]
async fn tldr_cfg_json_preserves_cfg_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "fn helper(flag: bool) { if flag { println!(\"ok\"); } }\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "cfg",
            "--lang",
            "rust",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "helper",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    let details = &payload["analysis"]["details"];

    assert_eq!(payload["action"], "cfg");
    assert_eq!(payload["analysis"]["kind"], "cfg");
    assert_eq!(details["symbol_query"], "helper");
    assert!(
        payload["summary"]
            .as_str()
            .is_some_and(|summary| summary.starts_with("cfg summary:"))
    );
    assert!(
        details["units"][0]["cfg_summary"]
            .as_str()
            .is_some_and(|summary| !summary.is_empty())
    );

    Ok(())
}

#[tokio::test]
async fn tldr_dfg_json_preserves_dfg_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(
        project.path().join("src/lib.rs"),
        "fn helper(input: i32) -> i32 { let value = input + 1; value }\n",
    )?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "dfg",
            "--lang",
            "rust",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "helper",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    let details = &payload["analysis"]["details"];

    assert_eq!(payload["action"], "dfg");
    assert_eq!(payload["analysis"]["kind"], "dfg");
    assert_eq!(details["symbol_query"], "helper");
    assert!(
        payload["summary"]
            .as_str()
            .is_some_and(|summary| summary.starts_with("dfg summary:"))
    );
    assert!(
        details["units"][0]["dfg_summary"]
            .as_str()
            .is_some_and(|summary| !summary.is_empty())
    );

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_notify_json_preserves_snapshot_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "notify",
            "src/lib.rs",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "notify");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["snapshot"]["dirty_files"], 1);
    assert!(
        payload["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_ping_json_preserves_status_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "ping",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "ping");
    assert_eq!(payload["status"], "ok");
    assert_eq!(payload["message"], "pong");

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_warm_json_preserves_snapshot_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "warm",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "warm");
    assert_eq!(payload["status"], "ok");
    assert!(payload["snapshot"].is_object());
    assert!(
        payload["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_snapshot_json_preserves_snapshot_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "snapshot",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "snapshot");
    assert_eq!(payload["status"], "ok");
    assert!(payload["snapshot"].is_object());
    assert!(payload["snapshot"]["dirty_files"].as_u64().is_some());

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_status_json_preserves_status_contract() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut notify_cmd = codex_command(codex_home.path())?;
    notify_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "notify",
            "src/lib.rs",
        ])
        .assert()
        .success();

    let mut status_cmd = codex_command(codex_home.path())?;
    let output = status_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "status",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "status");
    assert_eq!(payload["status"], "ok");
    assert!(payload["daemonStatus"].is_object());
    assert!(payload["snapshot"].is_object());
    assert_eq!(payload["daemonStatus"]["healthy"], true);
    assert!(payload["daemonStatus"]["socket_path"].is_string());
    assert!(payload["snapshot"]["dirty_files"].as_u64().is_some());

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_start_then_stop_json_cleans_up_process() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;
    let project_arg = project
        .path()
        .to_str()
        .expect("project path should be utf-8");

    let mut start_cmd = codex_command(codex_home.path())?;
    let start_output = start_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project_arg,
            "--json",
            "start",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let start_payload: serde_json::Value = serde_json::from_slice(&start_output)?;
    assert_eq!(start_payload["action"], "start");
    assert_eq!(start_payload["status"], "ok");
    assert_eq!(start_payload["daemonStatus"]["healthy"], true);

    let mut stop_cmd = codex_command(codex_home.path())?;
    let stop_output = stop_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project_arg,
            "--json",
            "stop",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stop_payload: serde_json::Value = serde_json::from_slice(&stop_output)?;
    assert_eq!(stop_payload["action"], "stop");
    assert_eq!(stop_payload["stopped"], true);
    assert!(
        stop_payload["message"]
            .as_str()
            .is_some_and(|message| !message.is_empty())
    );

    let mut stop_again_cmd = codex_command(codex_home.path())?;
    let stop_again_output = stop_again_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project_arg,
            "--json",
            "stop",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let stop_again_payload: serde_json::Value = serde_json::from_slice(&stop_again_output)?;
    assert_eq!(stop_again_payload["action"], "stop");
    assert_eq!(stop_again_payload["stopped"], false);

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_notify_then_snapshot_reflects_dirty_file_count() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut notify_cmd = codex_command(codex_home.path())?;
    notify_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "notify",
            "src/lib.rs",
        ])
        .assert()
        .success();

    let mut snapshot_cmd = codex_command(codex_home.path())?;
    let output = snapshot_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "snapshot",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "snapshot");
    assert_eq!(payload["snapshot"]["dirty_files"], 1);
    assert_eq!(payload["snapshot"]["reindex_pending"], true);

    Ok(())
}

#[tokio::test]
async fn tldr_daemon_notify_then_warm_then_status_clears_reindex_pending() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut notify_cmd = codex_command(codex_home.path())?;
    notify_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "notify",
            "src/lib.rs",
        ])
        .assert()
        .success();

    let mut warm_cmd = codex_command(codex_home.path())?;
    warm_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "warm",
        ])
        .assert()
        .success();

    let mut status_cmd = codex_command(codex_home.path())?;
    let output = status_cmd
        .args([
            "ztldr",
            "daemon",
            "--project",
            project
                .path()
                .to_str()
                .expect("project path should be utf-8"),
            "--json",
            "status",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "status");
    assert_eq!(payload["snapshot"]["reindex_pending"], false);
    assert_eq!(payload["snapshot"]["dirty_files"], 0);
    assert!(payload["reindexReport"].is_object());

    Ok(())
}

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
async fn tldr_structure_json_exposes_graph_details() -> Result<()> {
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
            "tldr",
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
async fn tldr_context_json_exposes_deduplicated_call_graph() -> Result<()> {
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
            "tldr",
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
                "tldr",
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
async fn tldr_impact_json_exposes_pdg_details() -> Result<()> {
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
            "tldr",
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
    let helper_node = details["nodes"]
        .as_array()
        .expect("nodes should be an array")
        .iter()
        .find(|node| node["id"] == "helper")
        .expect("helper node should exist");
    let calls_main_helper = edges
        .iter()
        .filter(|edge| edge["kind"] == "calls" && edge["from"] == "main" && edge["to"] == "helper")
        .count();

    assert_eq!(payload["analysis"]["kind"], "pdg");
    assert_eq!(payload["action"], "impact");
    assert_eq!(details["symbol_query"], "helper");
    assert_eq!(details["overview"]["incoming_edges"], 1);
    assert_eq!(helper_node["kind"], "function");
    assert_eq!(
        details["units"][0]["dfg_summary"],
        "params=0, locals=0, mutable bindings=0, assignments=0, references=1"
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
            "tldr",
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
    assert!(text.contains("language: rust"));
    assert!(text.contains("source: "));
    assert!(text.contains("support: DataFlow"));
    assert!(text.contains("fallback: structure + search"));
    assert!(text.contains("message: "));
    assert!(text.contains("summary: impact summary:"));
    assert!(text.contains("incoming [main]"));

    Ok(())
}

#[tokio::test]
async fn tldr_cfg_json_exposes_cfg_details() -> Result<()> {
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
            "tldr",
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
async fn tldr_dfg_json_exposes_dfg_details() -> Result<()> {
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
            "tldr",
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
async fn tldr_daemon_notify_json_exposes_snapshot_details() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args([
            "tldr",
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
async fn tldr_daemon_status_json_exposes_daemon_status_fields() -> Result<()> {
    let codex_home = TempDir::new()?;
    let project = TempDir::new()?;
    std::fs::create_dir_all(project.path().join("src"))?;
    std::fs::write(project.path().join("src/lib.rs"), "fn helper() {}\n")?;

    let mut notify_cmd = codex_command(codex_home.path())?;
    notify_cmd
        .args([
            "tldr",
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
            "tldr",
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

use std::path::Path;

use anyhow::Result;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
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
    assert_eq!(calls_main_helper, 1);
    assert_eq!(details["overview"]["incoming_edges"], 1);
    assert!(details["nodes"].as_array().is_some_and(|nodes| {
        nodes
            .iter()
            .any(|node| node["id"] == "helper" && node["kind"] == "function")
    }));

    Ok(())
}

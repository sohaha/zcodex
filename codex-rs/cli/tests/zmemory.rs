use std::path::Path;

use anyhow::Result;
use predicates::prelude::PredicateBooleanExt;
use predicates::prelude::predicate;
use pretty_assertions::assert_eq;
use tempfile::TempDir;

fn codex_command(codex_home: &Path) -> Result<assert_cmd::Command> {
    let mut cmd = assert_cmd::Command::new(codex_utils_cargo_bin::cargo_bin("codex")?);
    cmd.env("CODEX_HOME", codex_home);
    Ok(cmd)
}

fn run_json(codex_home: &Path, args: &[&str]) -> Result<serde_json::Value> {
    let output = codex_command(codex_home)?
        .args(args)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    serde_json::from_slice(&output).map_err(Into::into)
}

#[tokio::test]
async fn zmemory_help_renders() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut cmd = codex_command(codex_home.path())?;
    cmd.args(["zmemory", "--help"]).assert().success();
    Ok(())
}

#[tokio::test]
async fn zmemory_stats_json_works_on_empty_db() -> Result<()> {
    let codex_home = TempDir::new()?;
    let mut cmd = codex_command(codex_home.path())?;
    let output = cmd
        .args(["zmemory", "stats", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let payload: serde_json::Value = serde_json::from_slice(&output)?;
    assert_eq!(payload["action"], "stats");
    assert_eq!(payload["result"]["nodeCount"], 1);
    assert_eq!(payload["result"]["orphanedMemoryCount"], 0);
    assert_eq!(payload["result"]["deprecatedMemoryCount"], 0);
    assert_eq!(payload["result"]["aliasNodeCount"], 0);
    assert_eq!(payload["result"]["triggerNodeCount"], 0);
    Ok(())
}

#[tokio::test]
async fn zmemory_create_then_read_then_search_round_trip() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Salem profile memory",
        ])
        .assert()
        .success();

    let read_output = codex_command(codex_home.path())?
        .args(["zmemory", "read", "core://agent-profile", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let read_payload: serde_json::Value = serde_json::from_slice(&read_output)?;
    assert_eq!(read_payload["result"]["content"], "Salem profile memory");

    let search_output = codex_command(codex_home.path())?
        .args(["zmemory", "search", "profile", "--json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let search_payload: serde_json::Value = serde_json::from_slice(&search_output)?;
    assert_eq!(search_payload["result"]["matchCount"], 1);

    Ok(())
}

#[tokio::test]
async fn zmemory_create_supports_parent_uri_and_title() -> Result<()> {
    let codex_home = TempDir::new()?;

    let create_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "create",
            "--parent-uri",
            "core://",
            "--title",
            "agent-profile",
            "--content",
            "Salem profile memory",
            "--priority",
            "5",
            "--json",
        ],
    )?;
    assert_eq!(create_payload["action"], "create");
    assert_eq!(create_payload["result"]["uri"], "core://agent-profile");

    let read_payload = run_json(
        codex_home.path(),
        &["zmemory", "read", "core://agent-profile", "--json"],
    )?;
    assert_eq!(read_payload["result"]["content"], "Salem profile memory");

    Ok(())
}

#[tokio::test]
async fn zmemory_create_without_title_uses_auto_numbering() -> Result<()> {
    let codex_home = TempDir::new()?;

    let create_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "create",
            "--parent-uri",
            "core://",
            "--content",
            "Auto numbered memory",
            "--priority",
            "3",
            "--json",
        ],
    )?;
    assert_eq!(create_payload["result"]["uri"], "core://1");

    Ok(())
}

#[tokio::test]
async fn zmemory_update_delete_and_rebuild_round_trip() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Original profile memory",
        ])
        .assert()
        .success();

    let update_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "update",
            "core://agent-profile",
            "--content",
            "Updated profile memory",
            "--priority",
            "8",
            "--disclosure",
            "team",
            "--json",
        ],
    )?;
    assert_eq!(update_payload["action"], "update");
    assert_eq!(update_payload["result"]["contentChanged"], true);
    assert_eq!(update_payload["result"]["priority"], 8);

    let rebuild_payload = run_json(codex_home.path(), &["zmemory", "rebuild-search", "--json"])?;
    assert_eq!(rebuild_payload["action"], "rebuild-search");
    assert_eq!(rebuild_payload["result"]["documentCount"], 1);

    let delete_payload = run_json(
        codex_home.path(),
        &["zmemory", "delete-path", "core://agent-profile", "--json"],
    )?;
    assert_eq!(delete_payload["action"], "delete-path");
    assert_eq!(delete_payload["result"]["deletedPaths"], 1);

    let search_payload = run_json(
        codex_home.path(),
        &["zmemory", "search", "Updated", "--json"],
    )?;
    assert_eq!(search_payload["result"]["matchCount"], 0);

    Ok(())
}

#[tokio::test]
async fn zmemory_create_rejects_combined_uri_and_parent_uri() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--parent-uri",
            "core://",
            "--content",
            "Conflicting create",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "`uri` cannot be combined with `parentUri` or `title`",
        ));

    Ok(())
}

#[tokio::test]
async fn zmemory_update_patch_supports_old_and_new_string() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Original profile memory",
        ])
        .assert()
        .success();

    let update_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "update",
            "core://agent-profile",
            "--old-string",
            "Original profile memory",
            "--new-string",
            "Patched profile memory",
            "--json",
        ],
    )?;
    assert_eq!(update_payload["action"], "update");
    assert_eq!(update_payload["result"]["contentChanged"], true);
    assert_eq!(update_payload["result"]["uri"], "core://agent-profile");

    Ok(())
}

#[tokio::test]
async fn zmemory_update_append_supports_whitespace_only() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Original profile memory",
        ])
        .assert()
        .success();

    let update_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "update",
            "core://agent-profile",
            "--append",
            "   ",
            "--json",
        ],
    )?;
    assert_eq!(update_payload["action"], "update");
    assert_eq!(update_payload["result"]["contentChanged"], true);

    let read_payload = run_json(
        codex_home.path(),
        &["zmemory", "read", "core://agent-profile", "--json"],
    )?;
    assert_eq!(
        read_payload["result"]["content"],
        "Original profile memory   "
    );

    Ok(())
}

#[tokio::test]
async fn zmemory_update_metadata_only_changes_priority() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Original profile memory",
        ])
        .assert()
        .success();

    let update_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "update",
            "core://agent-profile",
            "--priority",
            "9",
            "--json",
        ],
    )?;
    assert_eq!(update_payload["action"], "update");
    assert_eq!(update_payload["result"]["contentChanged"], false);
    assert_eq!(update_payload["result"]["priority"], 9);

    Ok(())
}
#[tokio::test]
async fn zmemory_system_views_and_doctor_are_available() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://salem",
            "--content",
            "Profile for Salem",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "manage-triggers",
            "core://salem",
            "--add",
            "profile",
            "--add",
            "agent",
        ])
        .assert()
        .success();

    let boot_payload = run_json(
        codex_home.path(),
        &["zmemory", "read", "system://boot", "--json"],
    )?;
    assert_eq!(boot_payload["result"]["view"]["view"], "boot");
    assert_eq!(boot_payload["result"]["view"]["entryCount"], 1);

    let glossary_payload = run_json(
        codex_home.path(),
        &["zmemory", "read", "system://glossary", "--json"],
    )?;
    assert_eq!(glossary_payload["result"]["view"]["entryCount"], 2);

    let export_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "glossary", "--json"],
    )?;
    assert_eq!(export_payload["action"], "read");
    assert_eq!(export_payload["result"]["uri"], "system://glossary");
    assert_eq!(export_payload["result"]["view"]["entryCount"], 2);

    let doctor_payload = run_json(codex_home.path(), &["zmemory", "doctor", "--json"])?;
    assert_eq!(doctor_payload["action"], "doctor");
    assert_eq!(doctor_payload["result"]["healthy"], true);
    assert_eq!(doctor_payload["result"]["orphanedMemoryCount"], 0);
    assert_eq!(doctor_payload["result"]["deprecatedMemoryCount"], 0);
    assert!(
        doctor_payload["result"]["aliasNodeCount"]
            .as_i64()
            .unwrap_or(0)
            >= 0
    );
    assert!(
        doctor_payload["result"]["triggerNodeCount"]
            .as_i64()
            .unwrap_or(0)
            >= 0
    );

    Ok(())
}

#[tokio::test]
async fn zmemory_stats_and_doctor_surface_review_pressure() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://legacy",
            "--content",
            "Original profile memory",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "update",
            "core://legacy",
            "--append",
            " with fresh note",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://orphan",
            "--content",
            "Orphaned review source",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args(["zmemory", "delete-path", "core://orphan"])
        .assert()
        .success();

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "add-alias",
            "core://legacy/alias",
            "core://legacy",
        ])
        .assert()
        .success();

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://triggered",
            "--content",
            "Trigger node",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "manage-triggers",
            "core://triggered",
            "--add",
            "GraphService",
        ])
        .assert()
        .success();

    let stats_payload = run_json(codex_home.path(), &["zmemory", "stats", "--json"])?;
    assert_eq!(stats_payload["result"]["deprecatedMemoryCount"], 1);
    assert_eq!(stats_payload["result"]["orphanedMemoryCount"], 1);
    assert!(
        stats_payload["result"]["aliasNodeCount"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );
    assert!(
        stats_payload["result"]["triggerNodeCount"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );

    let doctor_payload = run_json(codex_home.path(), &["zmemory", "doctor", "--json"])?;
    assert_eq!(doctor_payload["result"]["healthy"], false);
    let issues = doctor_payload["result"]["issues"]
        .as_array()
        .expect("issues should be an array");
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "orphaned_memories")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "deprecated_memories_awaiting_review")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "alias_nodes_missing_triggers")
    );
    assert!(
        doctor_payload["result"]["aliasNodeCount"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );
    assert!(
        doctor_payload["result"]["triggerNodeCount"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );
    assert!(
        doctor_payload["result"]["aliasNodesMissingTriggers"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );

    Ok(())
}

#[tokio::test]
async fn zmemory_read_missing_memory_fails() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args(["zmemory", "read", "core://missing"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("memory not found: core://missing"));

    Ok(())
}

#[tokio::test]
async fn zmemory_update_without_changes_fails() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Profile memory",
        ])
        .assert()
        .success();

    codex_command(codex_home.path())?
        .args(["zmemory", "update", "core://agent-profile"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no changes requested"));

    Ok(())
}

#[tokio::test]
async fn zmemory_update_conflict_old_string_and_append() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Original profile memory",
        ])
        .assert()
        .success();

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "update",
            "core://agent-profile",
            "--old-string",
            "Original profile memory",
            "--append",
            "suffix",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("oldString")
                .or(predicate::str::contains("append"))
                .or(predicate::str::contains("cannot be combined")),
        );

    Ok(())
}

#[tokio::test]
async fn zmemory_export_supports_domain_and_recent_limit() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent-profile",
            "--content",
            "Profile memory",
        ])
        .assert()
        .success();

    let index_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "index", "--domain", "core", "--json"],
    )?;
    assert_eq!(index_payload["result"]["uri"], "system://index/core");
    assert_eq!(index_payload["result"]["view"]["domain"], "core");
    assert_eq!(index_payload["result"]["view"]["entryCount"], 1);

    let recent_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "recent", "--limit", "1", "--json"],
    )?;
    assert_eq!(recent_payload["result"]["uri"], "system://recent/1");
    assert_eq!(recent_payload["result"]["view"]["entryCount"], 1);

    Ok(())
}

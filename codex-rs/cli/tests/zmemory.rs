use std::path::Path;

use anyhow::Result;
use predicates::prelude::PredicateBooleanExt;
use predicates::prelude::predicate;
use pretty_assertions::assert_eq;
use serde_json::json;
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
async fn zmemory_help_localizes_nested_help_subcommand() -> Result<()> {
    let codex_home = TempDir::new()?;
    let output = codex_command(codex_home.path())?
        .args(["zmemory", "--help"])
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
async fn zmemory_export_help_lists_system_views() -> Result<()> {
    let codex_home = TempDir::new()?;
    codex_command(codex_home.path())?
        .args(["zmemory", "export", "--help"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("boot")
                .and(predicate::str::contains("defaults"))
                .and(predicate::str::contains("workspace"))
                .and(predicate::str::contains("paths")),
        );
    Ok(())
}

#[tokio::test]
async fn zmemory_export_memory_help_lists_uri_and_domain_flags() -> Result<()> {
    let codex_home = TempDir::new()?;
    codex_command(codex_home.path())?
        .args(["zmemory", "export-memory", "--help"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("--uri")
                .and(predicate::str::contains("--domain"))
                .and(predicate::str::contains("export-memory")),
        );
    Ok(())
}

#[tokio::test]
async fn zmemory_import_memory_help_shows_items_json() -> Result<()> {
    let codex_home = TempDir::new()?;
    codex_command(codex_home.path())?
        .args(["zmemory", "import-memory", "--help"])
        .assert()
        .success()
        .stderr(predicate::str::contains("--items-json"));
    Ok(())
}

#[tokio::test]
async fn zmemory_export_memory_json_supports_uri_scope() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://export-target",
            "--content",
            "Export target content",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "add-alias",
            "core://export-target-alias",
            "core://export-target",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "manage-triggers",
            "core://export-target",
            "--add",
            "export-keyword",
        ])
        .assert()
        .success();

    let payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "export-memory",
            "--uri",
            "core://export-target",
            "--json",
        ],
    )?;
    assert_eq!(payload["action"], "export");
    assert_eq!(payload["result"]["scope"]["type"], "uri");
    assert_eq!(payload["result"]["scope"]["value"], "core://export-target");
    assert_eq!(payload["result"]["count"], 1);
    assert_eq!(payload["result"]["items"][0]["uri"], "core://export-target");
    assert_eq!(
        payload["result"]["items"][0]["content"],
        "Export target content"
    );
    assert_eq!(
        payload["result"]["items"][0]["aliases"][0]["uri"],
        "core://export-target-alias"
    );
    assert_eq!(
        payload["result"]["items"][0]["keywords"][0],
        "export-keyword"
    );

    Ok(())
}

#[tokio::test]
async fn zmemory_export_memory_json_supports_domain_scope() -> Result<()> {
    let codex_home = TempDir::new()?;

    for uri in ["core://domain-one", "core://domain-two"] {
        codex_command(codex_home.path())?
            .args([
                "zmemory",
                "create",
                uri,
                "--content",
                "Domain export content",
            ])
            .assert()
            .success();
    }
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "add-alias",
            "core://domain-two-alias",
            "core://domain-two",
        ])
        .assert()
        .success();

    let payload = run_json(
        codex_home.path(),
        &["zmemory", "export-memory", "--domain", "core", "--json"],
    )?;
    assert_eq!(payload["action"], "export");
    assert_eq!(payload["result"]["scope"]["type"], "domain");
    assert_eq!(payload["result"]["scope"]["value"], "core");
    assert_eq!(payload["result"]["count"], 2);
    let items = payload["result"]["items"]
        .as_array()
        .expect("items should be array");
    assert_eq!(items.len(), 2);

    Ok(())
}

#[tokio::test]
async fn zmemory_import_memory_json_round_trips_aliases_and_keywords() -> Result<()> {
    let codex_home = TempDir::new()?;

    let import_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "import-memory",
            "--items-json",
            r#"[{"uri":"core://imported-memory","content":"Imported content","keywords":["import-keyword"],"aliases":[{"uri":"core://imported-memory-alias","priority":2,"disclosure":"imported alias"}]}]"#,
            "--json",
        ],
    )?;
    assert_eq!(import_payload["action"], "import");
    assert_eq!(import_payload["result"]["count"], 1);
    assert_eq!(
        import_payload["result"]["results"][0]["uri"],
        "core://imported-memory"
    );
    assert_eq!(import_payload["result"]["results"][0]["aliasCount"], 1);
    assert_eq!(import_payload["result"]["results"][0]["keywordCount"], 1);

    let read_payload = run_json(
        codex_home.path(),
        &["zmemory", "read", "core://imported-memory", "--json"],
    )?;
    assert_eq!(read_payload["result"]["content"], "Imported content");

    let search_payload = run_json(
        codex_home.path(),
        &["zmemory", "search", "import-keyword", "--json"],
    )?;
    assert_eq!(search_payload["result"]["matchCount"], 1);
    assert_eq!(
        search_payload["result"]["matches"][0]["uri"],
        "core://imported-memory"
    );

    let export_payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "export-memory",
            "--uri",
            "core://imported-memory",
            "--json",
        ],
    )?;
    assert_eq!(
        export_payload["result"]["items"][0]["aliases"][0]["uri"],
        "core://imported-memory-alias"
    );
    assert_eq!(
        export_payload["result"]["items"][0]["keywords"][0],
        "import-keyword"
    );

    Ok(())
}

#[tokio::test]
async fn zmemory_audit_help_lists_limit_flag() -> Result<()> {
    let codex_home = TempDir::new()?;
    codex_command(codex_home.path())?
        .args(["zmemory", "audit", "--help"])
        .assert()
        .success()
        .stderr(
            predicate::str::contains("--limit")
                .and(predicate::str::contains("--action"))
                .and(predicate::str::contains("--uri")),
        );
    Ok(())
}

#[tokio::test]
async fn zmemory_history_json_returns_version_chain() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://history-entry",
            "--content",
            "Initial version",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "update",
            "core://history-entry",
            "--append",
            " updated",
        ])
        .assert()
        .success();

    let payload = run_json(
        codex_home.path(),
        &["zmemory", "history", "core://history-entry", "--json"],
    )?;
    assert_eq!(payload["action"], "history");
    assert_eq!(payload["result"]["uri"], "core://history-entry");
    let versions = payload["result"]["versions"]
        .as_array()
        .expect("versions should be an array");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["content"], "Initial version updated");
    assert_eq!(versions[0]["deprecated"], false);
    assert_eq!(versions[1]["content"], "Initial version");
    assert_eq!(versions[1]["deprecated"], true);
    assert_eq!(versions[1]["migratedTo"], versions[0]["id"]);
    assert!(versions[0]["createdAt"].is_string());

    Ok(())
}

#[tokio::test]
async fn zmemory_batch_create_help_shows_items_json() -> Result<()> {
    let codex_home = TempDir::new()?;
    codex_command(codex_home.path())?
        .args(["zmemory", "batch-create", "--help"])
        .assert()
        .success()
        .stderr(predicate::str::contains("--items-json"));
    Ok(())
}

#[tokio::test]
async fn zmemory_batch_update_help_shows_items_json() -> Result<()> {
    let codex_home = TempDir::new()?;
    codex_command(codex_home.path())?
        .args(["zmemory", "batch-update", "--help"])
        .assert()
        .success()
        .stderr(predicate::str::contains("--items-json"));
    Ok(())
}

#[tokio::test]
async fn zmemory_batch_create_json_returns_results() -> Result<()> {
    let codex_home = TempDir::new()?;

    let payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "batch-create",
            "--items-json",
            r#"[{"uri":"core://batch-cli-one","content":"one"},{"uri":"core://batch-cli-two","content":"two"}]"#,
            "--json",
        ],
    )?;
    assert_eq!(payload["action"], "batch-create");
    assert_eq!(payload["result"]["count"], 2);
    assert_eq!(
        payload["result"]["results"][0]["uri"],
        "core://batch-cli-one"
    );
    assert_eq!(
        payload["result"]["results"][1]["uri"],
        "core://batch-cli-two"
    );

    let read = run_json(
        codex_home.path(),
        &["zmemory", "read", "core://batch-cli-two", "--json"],
    )?;
    assert_eq!(read["result"]["content"], "two");

    Ok(())
}

#[tokio::test]
async fn zmemory_batch_update_json_rolls_back_on_failure() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://batch-cli-update-one",
            "--content",
            "one",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://batch-cli-update-two",
            "--content",
            "two",
        ])
        .assert()
        .success();

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "batch-update",
            "--items-json",
            r#"[{"uri":"core://batch-cli-update-one","append":" updated"},{"uri":"core://batch-cli-update-missing","append":" updated"}]"#,
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("memory not found: core://batch-cli-update-missing"));

    let first = run_json(
        codex_home.path(),
        &["zmemory", "read", "core://batch-cli-update-one", "--json"],
    )?;
    let second = run_json(
        codex_home.path(),
        &["zmemory", "read", "core://batch-cli-update-two", "--json"],
    )?;
    assert_eq!(first["result"]["content"], "one");
    assert_eq!(second["result"]["content"], "two");

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
    assert_eq!(
        payload["result"]["dbPath"],
        payload["result"]["pathResolution"]["dbPath"]
    );
    assert_eq!(
        payload["result"]["reason"],
        payload["result"]["pathResolution"]["reason"]
    );
    Ok(())
}

#[tokio::test]
async fn zmemory_audit_json_returns_recent_entries() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://audit-entry",
            "--content",
            "Initial audit content",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "update",
            "core://audit-entry",
            "--append",
            " updated",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "add-alias",
            "core://audit-entry-alias",
            "core://audit-entry",
        ])
        .assert()
        .success();

    let payload = run_json(
        codex_home.path(),
        &["zmemory", "audit", "--limit", "2", "--json"],
    )?;
    assert_eq!(payload["action"], "audit");
    assert_eq!(payload["result"]["count"], 2);
    assert_eq!(payload["result"]["limit"], 2);
    let entries = payload["result"]["entries"]
        .as_array()
        .expect("entries should be an array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0]["action"], "add-alias");
    assert_eq!(entries[0]["uri"], "core://audit-entry-alias");
    assert_eq!(entries[1]["action"], "update");
    assert_eq!(entries[1]["uri"], "core://audit-entry");
    assert!(entries[0]["details"].is_object());
    assert!(entries[0]["createdAt"].is_string());
    let ids = entries
        .iter()
        .map(|entry| entry["id"].as_i64().expect("entry id should be integer"))
        .collect::<Vec<_>>();
    assert!(ids.windows(2).all(|pair| pair[0] > pair[1]));

    Ok(())
}

#[tokio::test]
async fn zmemory_audit_json_supports_action_and_uri_filters() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://audit-filter-entry",
            "--content",
            "Initial audit content",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "update",
            "core://audit-filter-entry",
            "--append",
            " updated",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "add-alias",
            "core://audit-filter-alias",
            "core://audit-filter-entry",
        ])
        .assert()
        .success();

    let payload = run_json(
        codex_home.path(),
        &[
            "zmemory",
            "audit",
            "--action",
            "add-alias",
            "--uri",
            "core://audit-filter-alias",
            "--json",
        ],
    )?;
    assert_eq!(payload["action"], "audit");
    assert_eq!(payload["result"]["count"], 1);
    assert_eq!(payload["result"]["auditAction"], "add-alias");
    assert_eq!(payload["result"]["uri"], "core://audit-filter-alias");
    assert_eq!(payload["result"]["entries"][0]["action"], "add-alias");
    assert_eq!(
        payload["result"]["entries"][0]["uri"],
        "core://audit-filter-alias"
    );

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
async fn zmemory_search_reports_valid_domains_and_normalizes_alias_queries() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .env("VALID_DOMAINS", "core,writer")
        .args([
            "zmemory",
            "create",
            "core://alias-seed",
            "--content",
            "Alias path search seed",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .env("VALID_DOMAINS", "core,writer")
        .args([
            "zmemory",
            "create",
            "writer://folder",
            "--content",
            "Writer folder",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .env("VALID_DOMAINS", "core,writer")
        .args([
            "zmemory",
            "add-alias",
            "writer://folder/mirror-note",
            "core://alias-seed",
            "--priority",
            "4",
        ])
        .assert()
        .success();

    let normalized = codex_command(codex_home.path())?
        .env("VALID_DOMAINS", "core,writer")
        .args([
            "zmemory",
            "search",
            "writer/folder/mirror-note",
            "--uri",
            "writer://",
            "--json",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let normalized_payload: serde_json::Value = serde_json::from_slice(&normalized)?;
    assert_eq!(normalized_payload["result"]["matchCount"], 1);
    assert_eq!(
        normalized_payload["result"]["matches"][0]["uri"],
        "writer://folder/mirror-note"
    );

    codex_command(codex_home.path())?
        .env("VALID_DOMAINS", "core,writer")
        .args(["zmemory", "search", "seed", "--uri", "unknown://"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "unknown domain 'unknown'. valid domains: core, writer, system",
        ));

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
async fn zmemory_delete_path_preserves_other_aliases() -> Result<()> {
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
        .args([
            "zmemory",
            "add-alias",
            "core://profile-mirror",
            "core://agent-profile",
        ])
        .assert()
        .success();

    run_json(
        codex_home.path(),
        &["zmemory", "delete-path", "core://profile-mirror", "--json"],
    )?;

    let read_payload = run_json(
        codex_home.path(),
        &["zmemory", "read", "core://agent-profile", "--json"],
    )?;
    assert_eq!(read_payload["result"]["content"], "Profile memory");

    let search_payload = run_json(
        codex_home.path(),
        &["zmemory", "search", "Profile", "--json"],
    )?;
    assert_eq!(search_payload["result"]["matchCount"], 1);
    assert_eq!(
        search_payload["result"]["matches"][0]["uri"],
        "core://agent-profile"
    );

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
            "core://agent",
            "--content",
            "Agent root",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent/coding_operating_manual",
            "--content",
            "Profile for agent",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "manage-triggers",
            "core://agent/coding_operating_manual",
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
    assert_eq!(
        boot_payload["result"]["view"]["bootRoles"][0]["role"],
        "agent_operating_manual"
    );
    assert_eq!(
        boot_payload["result"]["view"]["bootRoles"][0]["configured"],
        true
    );
    assert_eq!(boot_payload["result"]["view"]["unassignedUris"], json!([]));
    assert_eq!(boot_payload["result"]["view"]["missingUriCount"], 2);
    assert_eq!(
        boot_payload["result"]["view"]["entries"][0]["uri"],
        "core://agent/coding_operating_manual"
    );

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
            "--disclosure",
            "review/handoff",
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
            "create",
            "core://undisclosed",
            "--content",
            "Missing disclosure",
        ])
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
    assert_eq!(stats_payload["result"]["pathsMissingDisclosure"], 3);
    assert_eq!(stats_payload["result"]["disclosuresNeedingReview"], 1);

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
        issues
            .iter()
            .any(|issue| issue["code"] == "disclosures_need_review")
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
async fn zmemory_alias_view_reports_missing_trigger_nodes() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args(["zmemory", "create", "core://base", "--content", "Base node"])
        .assert()
        .success();

    codex_command(codex_home.path())?
        .args(["zmemory", "add-alias", "core://base/alias", "core://base"])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args(["zmemory", "add-alias", "core://base/alias-2", "core://base"])
        .assert()
        .success();

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://healthy",
            "--content",
            "Healthy node",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "add-alias",
            "core://healthy/alias",
            "core://healthy",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "manage-triggers",
            "core://healthy",
            "--add",
            "healthy",
        ])
        .assert()
        .success();

    let alias_payload = run_json(
        codex_home.path(),
        &["zmemory", "read", "system://alias", "--json"],
    )?;
    assert_eq!(alias_payload["result"]["view"]["view"], "alias");
    assert!(
        alias_payload["result"]["view"]["aliasNodeCount"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );
    assert!(
        alias_payload["result"]["view"]["aliasNodesMissingTriggers"]
            .as_i64()
            .unwrap_or(0)
            >= 1
    );
    let coverage_percent = alias_payload["result"]["view"]["coveragePercent"]
        .as_i64()
        .unwrap_or(100);
    let recommendations = alias_payload["result"]["view"]["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert!(!recommendations.is_empty());
    assert!(coverage_percent < 100);
    assert_eq!(recommendations[0]["action"], "manage-triggers");
    assert_eq!(recommendations[0]["reviewPriority"], "high");
    assert_eq!(recommendations[0]["priorityScore"], 103);
    assert!(
        recommendations[0]["command"]
            .as_str()
            .unwrap_or_default()
            .contains(&codex_utils_cli::format_launcher_command_from_env(&[
                "zmemory",
                "manage-triggers",
                "core://base",
            ]))
    );
    let entries = alias_payload["result"]["view"]["entries"]
        .as_array()
        .unwrap();
    assert_eq!(entries[0]["nodeUri"], "core://base");
    assert_eq!(entries[0]["reviewPriority"], "high");
    assert_eq!(entries[0]["priorityScore"], 103);
    assert!(
        entries
            .iter()
            .any(|entry| entry["missingTriggers"].as_bool().unwrap_or(false))
    );
    assert!(
        entries
            .iter()
            .any(|entry| entry["nodeUri"] == "core://healthy"
                && entry["reviewPriority"] == "low"
                && entry["priorityScore"] == 2)
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

    let paths_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "paths", "--domain", "core", "--json"],
    )?;
    assert_eq!(paths_payload["result"]["uri"], "system://paths/core");
    assert_eq!(paths_payload["result"]["view"]["view"], "paths");
    assert_eq!(paths_payload["result"]["view"]["entryCount"], 1);
    assert_eq!(
        paths_payload["result"]["view"]["entries"][0]["uri"],
        "core://agent-profile"
    );

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "add-alias",
            "alias://agent-profile",
            "core://agent-profile",
            "--json",
        ])
        .assert()
        .success();

    let alias_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "alias", "--limit", "1", "--json"],
    )?;
    assert_eq!(alias_payload["result"]["uri"], "system://alias/1");
    assert_eq!(alias_payload["result"]["view"]["view"], "alias");
    assert_eq!(alias_payload["result"]["view"]["entryCount"], 1);

    Ok(())
}

#[tokio::test]
async fn zmemory_export_supports_boot_defaults_and_workspace() -> Result<()> {
    let codex_home = TempDir::new()?;

    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent",
            "--content",
            "Agent root",
        ])
        .assert()
        .success();
    codex_command(codex_home.path())?
        .args([
            "zmemory",
            "create",
            "core://agent/coding_operating_manual",
            "--content",
            "Agent memory",
        ])
        .assert()
        .success();

    let boot_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "boot", "--limit", "1", "--json"],
    )?;
    assert_eq!(boot_payload["result"]["uri"], "system://boot");
    assert_eq!(boot_payload["result"]["view"]["view"], "boot");
    assert_eq!(
        boot_payload["result"]["view"]["bootRoles"][0]["configured"],
        true
    );
    assert_eq!(boot_payload["result"]["view"]["unassignedUris"], json!([]));

    let defaults_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "defaults", "--json"],
    )?;
    assert_eq!(defaults_payload["result"]["uri"], "system://defaults");
    assert_eq!(defaults_payload["result"]["view"]["view"], "defaults");
    assert_eq!(
        defaults_payload["result"]["view"]["bootRoles"][2]["role"],
        "collaboration_contract"
    );
    assert_eq!(
        defaults_payload["result"]["view"]["unassignedUris"],
        json!([])
    );

    let workspace_payload = run_json(
        codex_home.path(),
        &["zmemory", "export", "workspace", "--json"],
    )?;
    assert_eq!(workspace_payload["result"]["uri"], "system://workspace");
    assert_eq!(workspace_payload["result"]["view"]["view"], "workspace");
    assert_eq!(
        workspace_payload["result"]["view"]["bootRoles"][1]["configured"],
        true
    );
    assert_eq!(
        workspace_payload["result"]["view"]["unassignedUris"],
        json!([])
    );

    Ok(())
}

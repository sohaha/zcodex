use crate::config::ZmemoryConfig;
use crate::config::ZmemorySettings;
use crate::path_resolution::resolve_workspace_base_path;
use crate::path_resolution::resolve_zmemory_path;
use crate::tool_api::ZmemoryToolAction;
use crate::tool_api::ZmemoryToolCallParam;
use pretty_assertions::assert_eq;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;
use tempfile::TempDir;

fn config() -> (TempDir, ZmemoryConfig) {
    let dir = TempDir::new().expect("tempdir");
    let resolution =
        resolve_zmemory_path(dir.path(), dir.path(), None).expect("resolve zmemory path");
    let workspace_base = resolve_workspace_base_path(dir.path()).expect("resolve workspace base");
    let config = ZmemoryConfig::new(dir.path().to_path_buf(), workspace_base, resolution);
    (dir, config)
}

fn config_with_settings(settings: ZmemorySettings) -> (TempDir, ZmemoryConfig) {
    let dir = TempDir::new().expect("tempdir");
    let resolution =
        resolve_zmemory_path(dir.path(), dir.path(), None).expect("resolve zmemory path");
    let workspace_base = resolve_workspace_base_path(dir.path()).expect("resolve workspace base");
    let config = ZmemoryConfig::new_with_settings(
        dir.path().to_path_buf(),
        workspace_base,
        resolution,
        settings,
    );
    (dir, config)
}

#[test]
fn create_read_search_and_rebuild_round_trip() {
    let (_dir, config) = config();
    let create = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Stores agent profile memory".to_string()),
            priority: Some(5),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    assert_eq!(create["action"], "create");
    assert_eq!(create["result"]["uri"], "core://agent-profile");

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read should succeed");
    assert_eq!(read["result"]["content"], "Stores agent profile memory");

    let search = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");
    assert_eq!(search["result"]["matchCount"], 1);

    let rebuild = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::RebuildSearch,
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("rebuild should succeed");
    assert_eq!(rebuild["result"]["documentCount"], 1);
}

#[test]
fn create_supports_parent_uri_and_auto_numbering() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            parent_uri: Some("core://".to_string()),
            title: Some("agent".to_string()),
            content: Some("Stores agent profile memory".to_string()),
            priority: Some(5),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("named create should succeed");

    let numbered = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            parent_uri: Some("core://".to_string()),
            content: Some("Auto numbered memory".to_string()),
            priority: Some(3),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("auto-numbered create should succeed");

    assert_eq!(numbered["result"]["uri"], "core://1");

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read should succeed");
    assert_eq!(read["result"]["content"], "Stores agent profile memory");
}

#[test]
fn batch_create_creates_multiple_memories_in_one_transaction() {
    let (_dir, config) = config();
    let create = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::BatchCreate,
            items: Some(vec![
                json!({
                    "uri": "core://batch-one",
                    "content": "first memory",
                    "priority": 2
                }),
                json!({
                    "uri": "core://batch-two",
                    "content": "second memory"
                }),
            ]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("batch create should succeed");

    assert_eq!(create["action"], "batch-create");
    assert_eq!(create["result"]["count"], 2);
    assert_eq!(create["result"]["results"][0]["uri"], "core://batch-one");
    assert_eq!(create["result"]["results"][1]["uri"], "core://batch-two");
    assert_eq!(create["result"]["documentCount"], 2);

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://batch-two".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read should succeed");
    assert_eq!(read["result"]["content"], "second memory");
}

#[test]
fn batch_update_rolls_back_when_any_item_fails() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://batch-update-one".to_string()),
            content: Some("one".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create one should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://batch-update-two".to_string()),
            content: Some("two".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create two should succeed");

    let error = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::BatchUpdate,
            items: Some(vec![
                json!({
                    "uri": "core://batch-update-one",
                    "append": " updated"
                }),
                json!({
                    "uri": "core://batch-update-missing",
                    "append": " updated"
                }),
            ]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("batch update should fail");
    assert_eq!(
        error.to_string(),
        "memory not found: core://batch-update-missing"
    );

    let first = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://batch-update-one".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read one should succeed");
    let second = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://batch-update-two".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read two should succeed");
    assert_eq!(first["result"]["content"], "one");
    assert_eq!(second["result"]["content"], "two");
}

#[test]
fn history_returns_full_version_chain_sorted() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://history-node".to_string()),
            content: Some("initial".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://history-node".to_string()),
            append: Some(" #1".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("update should succeed");

    let history = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::History,
            uri: Some("core://history-node".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("history should succeed");

    let versions = history["result"]["versions"]
        .as_array()
        .expect("versions should be an array");
    assert_eq!(history["action"], "history");
    assert_eq!(history["result"]["uri"], "core://history-node");
    assert_eq!(versions.len(), 2);
    assert_eq!(versions[0]["content"], "initial #1");
    assert_eq!(versions[0]["deprecated"], false);
    assert_eq!(versions[1]["content"], "initial");
    assert_eq!(versions[1]["deprecated"], true);
    assert!(versions[0]["migratedTo"].is_null());
    assert_eq!(versions[1]["migratedTo"], versions[0]["id"]);
    assert!(versions[0]["createdAt"].is_string());
}

#[test]
fn create_rejects_conflicting_uri_modes_and_invalid_title() {
    let (_dir, config) = config();
    let conflict = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
            parent_uri: Some("core://".to_string()),
            content: Some("Stores agent profile memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("uri and parentUri should conflict");
    assert_eq!(
        conflict.to_string(),
        "`uri` cannot be combined with `parentUri` or `title`"
    );

    let invalid_title = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            parent_uri: Some("core://".to_string()),
            title: Some("bad title".to_string()),
            content: Some("Stores agent profile memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("invalid title should fail");
    assert_eq!(
        invalid_title.to_string(),
        "`title` may only contain ASCII letters, numbers, `_`, or `-`"
    );
}

#[test]
fn system_views_reflect_runtime_settings_without_changing_defaults() {
    let (_dir, config) = config_with_settings(ZmemorySettings::from_sources(
        Some(vec![
            "core".to_string(),
            "project".to_string(),
            "notes".to_string(),
        ]),
        Some(vec![
            "core://agent/coding_operating_manual".to_string(),
            "core://my_user/coding_preferences".to_string(),
            "core://agent/my_user/collaboration_contract".to_string(),
        ]),
        None,
        None,
    ));

    let workspace = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://workspace".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("workspace view should succeed");
    assert_eq!(
        workspace["result"]["view"]["validDomains"],
        json!(["core", "project", "notes"])
    );
    assert_eq!(
        workspace["result"]["view"]["coreMemoryUris"],
        json!([
            "core://agent/coding_operating_manual",
            "core://my_user/coding_preferences",
            "core://agent/my_user/collaboration_contract"
        ])
    );

    let boot = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://boot".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("boot view should succeed");
    assert_eq!(boot["result"]["view"]["configuredUriCount"], 3);
    assert_eq!(
        boot["result"]["view"]["configuredUris"],
        json!([
            "core://agent/coding_operating_manual",
            "core://my_user/coding_preferences",
            "core://agent/my_user/collaboration_contract"
        ])
    );

    let defaults = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://defaults".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("defaults view should succeed");
    assert_eq!(
        defaults["result"]["view"]["validDomains"],
        json!(["core", "project", "notes"])
    );
    assert_eq!(
        defaults["result"]["view"]["coreMemoryUris"],
        json!([
            "core://agent/coding_operating_manual",
            "core://my_user/coding_preferences",
            "core://agent/my_user/collaboration_contract"
        ])
    );
    assert_eq!(
        defaults["result"]["view"]["bootRoles"],
        json!([
            {
                "role": "agent_operating_manual",
                "uri": "core://agent/coding_operating_manual",
                "configured": true,
                "description": "The assistant's coding operating manual."
            },
            {
                "role": "user_preferences",
                "uri": "core://my_user/coding_preferences",
                "configured": true,
                "description": "Stable user coding preferences for this runtime profile."
            },
            {
                "role": "collaboration_contract",
                "uri": "core://agent/my_user/collaboration_contract",
                "configured": true,
                "description": "Shared long-term collaboration rules for coding tasks."
            }
        ])
    );
    assert_eq!(workspace["result"]["view"]["unassignedUris"], json!([]));
}

#[test]
fn workspace_boot_roles_keep_shape_for_partial_and_extra_profiles() {
    let (_partial_dir, partial_config) = config_with_settings(ZmemorySettings::from_sources(
        None,
        Some(vec![
            "core://agent/custom_manual".to_string(),
            "core://my_user/custom_preferences".to_string(),
        ]),
        None,
        None,
    ));

    let partial_workspace = crate::service::execute_action(
        &partial_config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://workspace".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("partial workspace view should succeed");
    assert_eq!(
        partial_workspace["result"]["view"]["bootRoles"],
        json!([
            {
                "role": "agent_operating_manual",
                "uri": "core://agent/custom_manual",
                "configured": true,
                "description": "The assistant's coding operating manual."
            },
            {
                "role": "user_preferences",
                "uri": "core://my_user/custom_preferences",
                "configured": true,
                "description": "Stable user coding preferences for this runtime profile."
            },
            {
                "role": "collaboration_contract",
                "uri": null,
                "configured": false,
                "description": "Shared long-term collaboration rules for coding tasks."
            }
        ])
    );
    assert_eq!(
        partial_workspace["result"]["view"]["unassignedUris"],
        json!([])
    );

    let (_extra_dir, extra_config) = config_with_settings(ZmemorySettings::from_sources(
        None,
        Some(vec![
            "core://agent/custom_manual".to_string(),
            "core://my_user/custom_preferences".to_string(),
            "core://agent/my_user/custom_contract".to_string(),
            "project://repo/architecture".to_string(),
        ]),
        None,
        None,
    ));

    let extra_workspace = crate::service::execute_action(
        &extra_config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://workspace".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("extra workspace view should succeed");
    assert_eq!(
        extra_workspace["result"]["view"]["bootRoles"][2]["uri"],
        "core://agent/my_user/custom_contract"
    );
    assert_eq!(
        extra_workspace["result"]["view"]["unassignedUris"],
        json!(["project://repo/architecture"])
    );
}

#[test]
fn alias_and_manage_triggers_are_visible_in_read() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://team".to_string()),
            content: Some("Team root".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("parent create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://salem".to_string()),
            content: Some("Profile for Salem".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://team/salem".to_string()),
            target_uri: Some("core://salem".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("alias should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://salem".to_string()),
            add: Some(vec!["Profile".to_string(), "Agent".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("manage triggers should succeed");

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://salem".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read should succeed");

    assert_eq!(read["result"]["aliasCount"], 2);
    assert_eq!(read["result"]["keywords"].as_array().map(Vec::len), Some(2));
}

#[test]
fn update_supports_patch_append_and_metadata_only_modes() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
            content: Some("Original memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");

    let update = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://agent".to_string()),
            old_string: Some("Original".to_string()),
            new_string: Some("Updated".to_string()),
            priority: Some(5),
            disclosure: Some("team".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("update should succeed");

    assert_eq!(update["result"]["contentChanged"], true);
    assert_eq!(update["result"]["priority"], 5);
    assert_eq!(update["result"]["disclosure"], "team");
    assert!(update["result"]["newMemoryId"].as_i64().is_some());

    let metadata_only = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://agent".to_string()),
            priority: Some(9),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("metadata-only update should succeed");
    assert_eq!(metadata_only["result"]["contentChanged"], false);
    assert!(metadata_only["result"]["newMemoryId"].is_null());
    assert_eq!(metadata_only["result"]["priority"], 9);

    let append = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://agent".to_string()),
            append: Some("   ".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("append update should succeed");
    assert_eq!(append["result"]["contentChanged"], true);

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read should succeed");
    assert_eq!(read["result"]["content"], "Updated memory   ");
    assert_eq!(read["result"]["priority"], 9);

    let search = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("Updated".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");
    assert_eq!(search["result"]["matchCount"], 1);
}

#[test]
fn update_rejects_conflicting_or_invalid_patch_modes() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
            content: Some("Original memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");

    let conflict = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://agent".to_string()),
            content: Some("Replacement".to_string()),
            append: Some("suffix".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("content and append should conflict");
    assert_eq!(
        conflict.to_string(),
        "`content` cannot be combined with `oldString`/`newString`/`append`"
    );

    let missing_new = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://agent".to_string()),
            old_string: Some("Original".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("oldString without newString should fail");
    assert_eq!(
        missing_new.to_string(),
        "`newString` is required when `oldString` is provided"
    );

    let duplicate_patch = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://agent".to_string()),
            old_string: Some("m".to_string()),
            new_string: Some("M".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("ambiguous patch should fail");
    assert_eq!(
        duplicate_patch.to_string(),
        "`oldString` matched multiple locations; provide a more specific value"
    );

    let empty_append = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://agent".to_string()),
            append: Some(String::new()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("empty append should fail");
    assert_eq!(empty_append.to_string(), "`append` cannot be empty");
}

#[test]
fn delete_path_removes_last_reference_from_search() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://obsolete".to_string()),
            content: Some("Obsolete memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");

    let delete = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::DeletePath,
            uri: Some("core://obsolete".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("delete should succeed");
    assert_eq!(delete["result"]["deletedPaths"], 1);
    assert_eq!(delete["result"]["deletedEdges"], 1);
    assert_eq!(delete["result"]["deprecatedNodes"], 1);

    let search = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("Obsolete".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");
    assert_eq!(search["result"]["matchCount"], 0);
}

#[test]
fn delete_path_preserves_other_alias_paths_for_same_node() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent-profile".to_string()),
            content: Some("Profile memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://profile-mirror".to_string()),
            target_uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add alias should succeed");

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::DeletePath,
            uri: Some("core://profile-mirror".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("delete should succeed");

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://agent-profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("primary path should remain readable");
    assert_eq!(read["result"]["content"], "Profile memory");

    let search = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("Profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");
    assert_eq!(search["result"]["matchCount"], 1);
    assert_eq!(
        search["result"]["matches"][0]["uri"],
        "core://agent-profile"
    );
}

#[test]
fn system_views_reflect_index_recent_and_glossary() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent".to_string()),
            content: Some("Agent root".to_string()),
            priority: Some(8),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create parent should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://agent/coding_operating_manual".to_string()),
            content: Some("Profile for agent".to_string()),
            priority: Some(7),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://agent/coding_operating_manual".to_string()),
            add: Some(vec!["profile".to_string(), "agent".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("manage triggers should succeed");

    let boot = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://boot".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("boot view should succeed");
    assert_eq!(boot["result"]["view"]["view"], "boot");
    assert_eq!(boot["result"]["view"]["bootHealthy"], false);
    assert_eq!(boot["result"]["view"]["entryCount"], 1);
    assert_eq!(
        boot["result"]["view"]["presentUris"],
        json!(["core://agent/coding_operating_manual"])
    );
    assert_eq!(
        boot["result"]["view"]["bootRoles"],
        json!([
            {
                "role": "agent_operating_manual",
                "uri": "core://agent/coding_operating_manual",
                "configured": true,
                "description": "The assistant's coding operating manual."
            },
            {
                "role": "user_preferences",
                "uri": "core://my_user/coding_preferences",
                "configured": true,
                "description": "Stable user coding preferences for this runtime profile."
            },
            {
                "role": "collaboration_contract",
                "uri": "core://agent/my_user/collaboration_contract",
                "configured": true,
                "description": "Shared long-term collaboration rules for coding tasks."
            }
        ])
    );
    assert_eq!(boot["result"]["view"]["unassignedUris"], json!([]));
    assert_eq!(boot["result"]["view"]["missingUriCount"], 2);
    assert_eq!(
        boot["result"]["view"]["entries"][0]["uri"],
        "core://agent/coding_operating_manual"
    );
    assert_eq!(
        boot["result"]["view"]["anchors"][0]["uri"],
        "core://agent/coding_operating_manual"
    );
    assert_eq!(
        boot["result"]["view"]["anchors"][0]["role"],
        "agent_operating_manual"
    );
    assert_eq!(boot["result"]["view"]["anchors"][0]["exists"], true);
    assert_eq!(
        boot["result"]["view"]["anchors"][1]["uri"],
        "core://my_user/coding_preferences"
    );
    assert_eq!(
        boot["result"]["view"]["anchors"][1]["role"],
        "user_preferences"
    );
    assert_eq!(boot["result"]["view"]["anchors"][1]["exists"], false);
    assert_eq!(
        boot["result"]["view"]["missingUris"][0],
        "core://my_user/coding_preferences"
    );
    assert_eq!(
        boot["result"]["view"]["missingUris"][1],
        "core://agent/my_user/collaboration_contract"
    );

    let defaults = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://defaults".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("defaults view should succeed");
    assert_eq!(defaults["result"]["view"]["view"], "defaults");
    assert_eq!(defaults["result"]["view"]["unassignedUris"], json!([]));
    assert_eq!(
        defaults["result"]["view"]["bootContract"]["entriesListOnlyPresentAnchors"],
        true
    );
    assert_eq!(
        defaults["result"]["view"]["bootContract"]["missingUrisAreAuthoritative"],
        true
    );
    assert_eq!(
        defaults["result"]["view"]["bootContract"]["roles"],
        defaults["result"]["view"]["bootRoles"]
    );
    assert_eq!(
        defaults["result"]["view"]["defaultPathPolicy"]["mode"],
        "projectScoped"
    );
    assert_eq!(
        defaults["result"]["view"]["defaultPathPolicy"]["dbPath"],
        json!(
            config
                .codex_home()
                .join("zmemory")
                .join("projects")
                .join(
                    config
                        .path_resolution()
                        .workspace_key
                        .as_deref()
                        .expect("workspace key")
                )
                .join("zmemory.db")
                .display()
                .to_string()
        )
    );
    assert_eq!(
        defaults["result"]["view"]["defaultPathPolicy"]["workspaceKey"],
        json!(config.path_resolution().workspace_key.clone())
    );

    let workspace = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://workspace".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("workspace view should succeed");
    assert_eq!(workspace["result"]["view"]["view"], "workspace");
    assert_eq!(workspace["result"]["view"]["source"], "projectScoped");
    assert_eq!(workspace["result"]["view"]["hasExplicitZmemoryPath"], false);
    assert_eq!(workspace["result"]["view"]["dbPathDiffers"], false);
    assert_eq!(
        workspace["result"]["view"]["defaultWorkspaceKey"],
        json!(config.path_resolution().workspace_key.clone())
    );
    assert_eq!(
        workspace["result"]["view"]["defaultDbPath"],
        workspace["result"]["view"]["dbPath"]
    );
    assert_eq!(
        workspace["result"]["view"]["workspaceBase"],
        json!(config.workspace_base().display().to_string())
    );
    assert_eq!(workspace["result"]["view"]["bootHealthy"], false);
    assert_eq!(
        workspace["result"]["view"]["bootRoles"],
        boot["result"]["view"]["bootRoles"]
    );
    assert_eq!(workspace["result"]["view"]["unassignedUris"], json!([]));

    let index = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://index".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("index view should succeed");
    assert_eq!(index["result"]["view"]["totalCount"], 2);

    let recent = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://recent".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("recent view should succeed");
    assert_eq!(recent["result"]["view"]["entryCount"], 2);

    let glossary = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://glossary".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("glossary view should succeed");
    assert_eq!(glossary["result"]["view"]["entryCount"], 2);

    let index_by_domain = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://index/core".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("domain-scoped index should succeed");
    assert_eq!(index_by_domain["result"]["view"]["view"], "index");
    assert_eq!(index_by_domain["result"]["view"]["domain"], "core");
    assert_eq!(index_by_domain["result"]["view"]["entryCount"], 2);

    let paths = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://paths".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("paths view should succeed");
    assert_eq!(paths["result"]["view"]["view"], "paths");
    assert_eq!(paths["result"]["view"]["entryCount"], 2);
    assert_eq!(paths["result"]["view"]["entries"][0]["uri"], "core://agent");
    assert_eq!(paths["result"]["view"]["entries"][0]["path"], "agent");

    let recent_with_path_limit = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://recent/1".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("path-limited recent view should succeed");
    assert_eq!(recent_with_path_limit["result"]["view"]["view"], "recent");
    assert_eq!(recent_with_path_limit["result"]["view"]["entryCount"], 1);

    let clamped_alias_limit = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://alias/999".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("oversized alias limit should clamp");
    assert_eq!(clamped_alias_limit["result"]["view"]["view"], "alias");
}

#[test]
fn recent_view_orders_distinct_nodes_by_real_memory_update_time() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://older".to_string()),
            content: Some("Older memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("older create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://newer".to_string()),
            content: Some("Newer memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("newer create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://newer-mirror".to_string()),
            target_uri: Some("core://newer".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("alias create should succeed");

    let conn = Connection::open(config.db_path()).expect("open sqlite db");
    conn.execute(
        "UPDATE memories
         SET created_at = ?1
         WHERE node_uuid = (
            SELECT e.child_uuid
            FROM edges e
            JOIN paths p ON p.edge_id = e.id
            WHERE p.domain = 'core' AND p.path = 'older'
         ) AND deprecated = FALSE",
        params!["2024-01-01 00:00:00"],
    )
    .expect("update older timestamp");
    conn.execute(
        "UPDATE memories
         SET created_at = ?1
         WHERE node_uuid = (
            SELECT e.child_uuid
            FROM edges e
            JOIN paths p ON p.edge_id = e.id
            WHERE p.domain = 'core' AND p.path = 'newer'
         ) AND deprecated = FALSE",
        params!["2024-01-02 00:00:00"],
    )
    .expect("update newer timestamp");
    drop(conn);

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::RebuildSearch,
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("rebuild should succeed");

    let recent = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://recent".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("recent view should succeed");
    assert_eq!(recent["result"]["view"]["entryCount"], 2);
    assert_eq!(
        recent["result"]["view"]["entries"][0]["uri"],
        "core://newer"
    );
    assert_eq!(
        recent["result"]["view"]["entries"][1]["uri"],
        "core://older"
    );
}

#[test]
fn invalid_domains_are_rejected_and_system_writes_are_blocked() {
    let (_dir, config) = config_with_settings(ZmemorySettings::from_env_vars(
        Some("core,notes".to_string()),
        None,
    ));

    let invalid_domain = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("writer://draft".to_string()),
            content: Some("unsupported".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("invalid domain should fail");
    assert_eq!(
        invalid_domain.to_string(),
        "unknown domain 'writer'. valid domains: core, notes, system, alias"
    );

    let invalid_index = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://index/writer".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("invalid index domain should fail");
    assert_eq!(
        invalid_index.to_string(),
        "unknown domain 'writer'. valid domains: core, notes, system, alias"
    );

    let invalid_system_view = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://nope".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("unknown system view should fail");
    assert_eq!(
        invalid_system_view.to_string(),
        "unknown system view `nope`. supported views: boot, defaults, workspace, index, index/<domain>, paths, paths/<domain>, recent, recent/<n>, glossary, alias, alias/<n>"
    );

    let system_write = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("system://boot-note".to_string()),
            content: Some("forbidden".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("system writes should fail");
    assert_eq!(system_write.to_string(), "system domain is read-only");
}

#[test]
fn stats_and_doctor_surface_review_pressure() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://legacy".to_string()),
            content: Some("Original profile memory".to_string()),
            disclosure: Some("review/handoff".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://legacy".to_string()),
            append: Some(" with fresh note".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("update should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://orphan".to_string()),
            content: Some("Orphaned review source".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::DeletePath,
            uri: Some("core://orphan".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("delete-path should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://undisclosed".to_string()),
            content: Some("Missing disclosure".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("undisclosed create should succeed");

    let stats = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Stats,
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("stats should succeed");
    assert_eq!(stats["result"]["deprecatedMemoryCount"], 1);
    assert_eq!(stats["result"]["orphanedMemoryCount"], 1);
    assert_eq!(stats["result"]["pathsMissingDisclosure"], 1);
    assert_eq!(stats["result"]["disclosuresNeedingReview"], 1);
    assert_eq!(stats["result"]["auditLogCount"], 5);
    assert!(stats["result"]["latestAuditAt"].is_string());
    assert_eq!(stats["result"]["auditActionCounts"]["create"], 3);
    assert_eq!(stats["result"]["auditActionCounts"]["update"], 1);
    assert_eq!(stats["result"]["auditActionCounts"]["delete-path"], 1);
    assert_eq!(
        stats["result"]["pathResolution"]["dbPath"],
        json!(config.db_path().display().to_string())
    );
    assert_eq!(
        stats["result"]["dbPath"],
        stats["result"]["pathResolution"]["dbPath"]
    );
    assert_eq!(
        stats["result"]["workspaceKey"],
        stats["result"]["pathResolution"]["workspaceKey"]
    );
    assert_eq!(
        stats["result"]["source"],
        stats["result"]["pathResolution"]["source"]
    );
    assert_eq!(
        stats["result"]["reason"],
        stats["result"]["pathResolution"]["reason"]
    );
    assert_eq!(stats["result"]["pathResolution"].get("canonicalBase"), None);
    assert_eq!(
        sorted_object_keys(&stats["result"]["pathResolution"]),
        vec!["dbPath", "reason", "source", "workspaceKey"]
    );

    let doctor = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Doctor,
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("doctor should succeed");
    assert_eq!(doctor["result"]["healthy"], false);
    assert_eq!(
        doctor["result"]["pathResolution"]["dbPath"],
        json!(config.db_path().display().to_string())
    );
    assert_eq!(
        doctor["result"]["dbPath"],
        doctor["result"]["pathResolution"]["dbPath"]
    );
    assert_eq!(
        doctor["result"]["workspaceKey"],
        doctor["result"]["pathResolution"]["workspaceKey"]
    );
    assert_eq!(
        doctor["result"]["source"],
        doctor["result"]["pathResolution"]["source"]
    );
    assert_eq!(
        doctor["result"]["reason"],
        doctor["result"]["pathResolution"]["reason"]
    );
    assert_eq!(
        doctor["result"]["pathResolution"].get("canonicalBase"),
        None
    );
    assert_eq!(
        sorted_object_keys(&doctor["result"]["pathResolution"]),
        vec!["dbPath", "reason", "source", "workspaceKey"]
    );
    assert_eq!(doctor["result"]["stats"]["auditLogCount"], 5);
    assert!(doctor["result"]["stats"]["latestAuditAt"].is_string());
    assert_eq!(doctor["result"]["stats"]["auditActionCounts"]["create"], 3);
    assert_eq!(doctor["result"]["stats"]["auditActionCounts"]["update"], 1);
    assert_eq!(
        doctor["result"]["stats"]["auditActionCounts"]["delete-path"],
        1
    );
    let issues = doctor["result"]["issues"]
        .as_array()
        .expect("issues should be an array");
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "deprecated_memories_awaiting_review")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "orphaned_memories")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "disclosures_need_review")
    );
}

#[test]
fn write_actions_append_audit_log_entries() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://audit_target".to_string()),
            content: Some("initial memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://audit_target".to_string()),
            append: Some(" updated".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("update should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://audit_target_alias".to_string()),
            target_uri: Some("core://audit_target".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add alias should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://audit_target".to_string()),
            add: Some(vec!["audit".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("manage triggers should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::DeletePath,
            uri: Some("core://audit_target_alias".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("delete alias should succeed");

    let stats = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Stats,
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("stats should succeed");
    assert_eq!(stats["result"]["auditLogCount"], 5);
    assert!(stats["result"]["latestAuditAt"].is_string());
    assert_eq!(stats["result"]["auditActionCounts"]["create"], 1);
    assert_eq!(stats["result"]["auditActionCounts"]["update"], 1);
    assert_eq!(stats["result"]["auditActionCounts"]["add-alias"], 1);
    assert_eq!(stats["result"]["auditActionCounts"]["manage-triggers"], 1);
    assert_eq!(stats["result"]["auditActionCounts"]["delete-path"], 1);

    let conn = Connection::open(config.db_path()).expect("open db");
    let rows = conn
        .prepare("SELECT action, uri, details FROM audit_log ORDER BY id ASC")
        .expect("prepare audit query")
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .expect("query audit rows")
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect("collect audit rows");
    assert_eq!(
        rows.len(),
        stats["result"]["auditLogCount"].as_u64().unwrap_or(0) as usize
    );
    let first_details = serde_json::from_str::<Value>(&rows[0].2).expect("details should be json");
    assert!(first_details.is_object());

    assert_eq!(
        rows.iter()
            .map(|(action, uri, _details)| (action.clone(), uri.clone()))
            .collect::<Vec<_>>(),
        vec![
            (
                "create".to_string(),
                Some("core://audit_target".to_string())
            ),
            (
                "update".to_string(),
                Some("core://audit_target".to_string())
            ),
            (
                "add-alias".to_string(),
                Some("core://audit_target_alias".to_string())
            ),
            (
                "manage-triggers".to_string(),
                Some("core://audit_target".to_string())
            ),
            (
                "delete-path".to_string(),
                Some("core://audit_target_alias".to_string())
            ),
        ]
    );
}

#[test]
fn audit_action_returns_recent_entries_in_desc_order() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://audit_feed".to_string()),
            content: Some("initial memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://audit_feed".to_string()),
            append: Some(" updated".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("update should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://audit_feed_alias".to_string()),
            target_uri: Some("core://audit_feed".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add alias should succeed");

    let audit = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Audit,
            limit: Some(3),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("audit should succeed");

    assert_eq!(audit["action"], "audit");
    assert_eq!(audit["result"]["count"], 3);
    assert_eq!(audit["result"]["limit"], 3);
    assert_eq!(
        audit["result"]["entries"]
            .as_array()
            .expect("entries should be an array")
            .len(),
        3
    );
    assert_eq!(audit["result"]["entries"][0]["action"], "add-alias");
    assert_eq!(
        audit["result"]["entries"][0]["uri"],
        "core://audit_feed_alias"
    );
    assert_eq!(audit["result"]["entries"][1]["action"], "update");
    assert_eq!(audit["result"]["entries"][1]["uri"], "core://audit_feed");
    assert_eq!(audit["result"]["entries"][2]["action"], "create");
    assert_eq!(audit["result"]["entries"][2]["uri"], "core://audit_feed");
    assert!(audit["result"]["entries"][0]["details"].is_object());
    assert!(audit["result"]["entries"][0]["createdAt"].is_string());

    let ids = audit["result"]["entries"]
        .as_array()
        .expect("entries should be an array")
        .iter()
        .map(|entry| {
            entry["id"]
                .as_i64()
                .expect("audit entry id should be an integer")
        })
        .collect::<Vec<_>>();
    assert!(ids.windows(2).all(|pair| pair[0] > pair[1]));
}

#[test]
fn audit_action_supports_action_and_uri_filters() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://audit_filter_target".to_string()),
            content: Some("initial memory".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Update,
            uri: Some("core://audit_filter_target".to_string()),
            append: Some(" updated".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("update should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://audit_filter_alias".to_string()),
            target_uri: Some("core://audit_filter_target".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add alias should succeed");

    let audit = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Audit,
            limit: Some(5),
            audit_action: Some("add-alias".to_string()),
            uri: Some("core://audit_filter_alias".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("filtered audit should succeed");

    assert_eq!(audit["action"], "audit");
    assert_eq!(audit["result"]["count"], 1);
    assert_eq!(audit["result"]["auditAction"], "add-alias");
    assert_eq!(audit["result"]["uri"], "core://audit_filter_alias");
    assert_eq!(audit["result"]["entries"][0]["action"], "add-alias");
    assert_eq!(
        audit["result"]["entries"][0]["uri"],
        "core://audit_filter_alias"
    );
}

#[test]
fn search_matches_alias_via_separator_normalized_query() {
    let (_dir, config) = config_with_settings(ZmemorySettings::from_env_vars(
        Some("core,writer".to_string()),
        None,
    ));

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://alias-seed".to_string()),
            content: Some("Alias path search seed".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("writer://folder".to_string()),
            content: Some("Writer folder".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("writer folder should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("writer://folder/mirror-note".to_string()),
            target_uri: Some("core://alias-seed".to_string()),
            priority: Some(4),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("alias should succeed");

    let exact = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            uri: Some("writer://".to_string()),
            query: Some("writer://folder/mirror-note".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("exact search should succeed");
    assert_eq!(exact["result"]["matchCount"], 1);
    assert_eq!(
        exact["result"]["matches"][0]["uri"],
        "writer://folder/mirror-note"
    );

    let normalized = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            uri: Some("writer://".to_string()),
            query: Some("writer/folder/mirror-note".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("normalized search should succeed");
    assert_eq!(normalized["result"]["matchCount"], 1);
    assert_eq!(
        normalized["result"]["matches"][0]["uri"],
        "writer://folder/mirror-note"
    );
}

#[test]
fn search_dedupes_aliases_and_orders_by_priority_then_path_length() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://ranking_primary".to_string()),
            content: Some("omega delta".to_string()),
            priority: Some(3),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("primary create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://aliases".to_string()),
            content: Some("Aliases root".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("aliases root create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://aliases/ranking_primary_alias".to_string()),
            target_uri: Some("core://ranking_primary".to_string()),
            priority: Some(3),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("alias create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://a".to_string()),
            content: Some("omega delta".to_string()),
            priority: Some(1),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("short path create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://longer_path".to_string()),
            content: Some("omega delta".to_string()),
            priority: Some(1),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("long path create should succeed");

    let search = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("omega delta".to_string()),
            limit: Some(3),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");

    let matches = search["result"]["matches"]
        .as_array()
        .expect("matches should be an array");
    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0]["uri"], "core://a");
    assert_eq!(matches[1]["uri"], "core://longer_path");
    assert_eq!(matches[2]["uri"], "core://ranking_primary");
}

#[test]
fn search_uses_bm25_after_priority_in_sql_ordering() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://ranking_exact".to_string()),
            content: Some("omega delta".to_string()),
            priority: Some(2),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("exact create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://ranking_partial".to_string()),
            content: Some("omega middle filler words delta".to_string()),
            priority: Some(2),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("partial create should succeed");

    let search = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("omega delta".to_string()),
            limit: Some(2),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");

    let matches = search["result"]["matches"]
        .as_array()
        .expect("matches should be an array");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0]["uri"], "core://ranking_exact");
    assert_eq!(matches[1]["uri"], "core://ranking_partial");
}

#[test]
fn search_snippet_prefers_literal_then_token_then_fallback() {
    let (_dir, config) = config_with_settings(ZmemorySettings::from_env_vars(
        Some("core,writer".to_string()),
        None,
    ));

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://snippet_literal".to_string()),
            content: Some(format!(
                "prefix {} GraphService exact phrase keeps literal hits focused {}",
                "x".repeat(40),
                "y".repeat(40)
            )),
            priority: Some(1),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("literal create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://snippet_token".to_string()),
            content: Some("mirror token keeps hits focused".to_string()),
            priority: Some(2),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("token create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("writer://folder".to_string()),
            content: Some("Writer folder".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("writer folder create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("writer://folder/mirror-note".to_string()),
            target_uri: Some("core://snippet_token".to_string()),
            priority: Some(2),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("token alias should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://snippet_fallback".to_string()),
            content: Some("z".repeat(120)),
            priority: Some(3),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("fallback create should succeed");

    let literal = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("GraphService exact phrase".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("literal search should succeed");
    let literal_snippet = literal["result"]["matches"][0]["snippet"]
        .as_str()
        .expect("literal snippet should exist");
    assert!(literal_snippet.contains("<mark>GraphService exact phrase</mark>"));
    assert!(literal_snippet.contains("..."));

    let token = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            uri: Some("writer://".to_string()),
            query: Some("writer://folder/mirror-note".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("token search should succeed");
    assert_eq!(
        token["result"]["matches"][0]["snippet"],
        "<mark>mirror</mark> token keeps hits focused"
    );

    let fallback = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("snippet_fallback".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("fallback search should succeed");
    assert_eq!(
        fallback["result"]["matches"][0]["snippet"],
        format!("{}...", "z".repeat(80))
    );
}

#[test]
fn search_snippet_falls_back_to_content_for_disclosure_and_uri_hits() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://snippet_field_contract".to_string()),
            content: Some(
                "content snippet fallback keeps search previews rooted in content".to_string(),
            ),
            disclosure: Some("edge recall notice".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");

    let disclosure = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("edge recall".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("disclosure search should succeed");
    assert_eq!(
        disclosure["result"]["matches"][0]["snippet"],
        "content snippet fallback keeps search previews rooted in content"
    );

    let uri = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("core://snippet_field_contract".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("uri search should succeed");
    assert_eq!(
        uri["result"]["matches"][0]["snippet"],
        "content snippet fallback keeps search previews rooted in content"
    );
}

#[test]
fn search_snippet_preserves_multibyte_boundaries() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://snippet_multibyte_fallback".to_string()),
            content: Some("量".repeat(90)),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("fallback create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://snippet_multibyte_literal".to_string()),
            content: Some(format!("前缀{}GraphService后缀", "量".repeat(40))),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("literal create should succeed");

    let fallback = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("snippet_multibyte_fallback".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("fallback search should succeed");
    assert_eq!(
        fallback["result"]["matches"][0]["snippet"],
        format!("{}...", "量".repeat(80))
    );

    let literal = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("GraphService".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("literal search should succeed");
    let literal_snippet = literal["result"]["matches"][0]["snippet"]
        .as_str()
        .expect("literal snippet should exist");
    assert!(literal_snippet.contains("<mark>GraphService</mark>后缀"));
}

#[test]
fn glossary_add_and_remove_refresh_search_contract() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://anchor_refresh_contract".to_string()),
            content: Some("超导量子系统比特控制与协作".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");

    let before_add = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("子系统比".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");
    assert_eq!(before_add["result"]["matchCount"], 0);

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://anchor_refresh_contract".to_string()),
            add: Some(vec!["子系统比".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add trigger should succeed");

    let after_add = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("子系统比".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");
    assert_eq!(after_add["result"]["matchCount"], 1);
    assert_eq!(
        after_add["result"]["matches"][0]["uri"],
        "core://anchor_refresh_contract"
    );

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://anchor_refresh_contract".to_string()),
            remove: Some(vec!["子系统比".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("remove trigger should succeed");

    let after_remove = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("子系统比".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("search should succeed");
    assert_eq!(after_remove["result"]["matchCount"], 0);
}

#[test]
fn search_uses_token_boundaries_instead_of_raw_cjk_substrings() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://cjk_search".to_string()),
            content: Some("超导量子系统比特控制与量子比特协作".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://cjk_search".to_string()),
            add: Some(vec!["量子比特".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("manage triggers should succeed");

    let hit = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("量子比特".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("hit search should succeed");
    assert_eq!(hit["result"]["matchCount"], 1);
    assert_eq!(hit["result"]["matches"][0]["uri"], "core://cjk_search");

    let miss = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Search,
            query: Some("子系统比".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("miss search should succeed");
    assert_eq!(miss["result"]["matchCount"], 0);
}

#[test]
fn alias_view_includes_priority_reasons_and_suggested_keywords() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://hub".to_string()),
            content: Some("Hub".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("hub create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://zone".to_string()),
            content: Some("Zone".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("zone create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://project-alpha".to_string()),
            content: Some("Project note".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://hub/launch-plan".to_string()),
            target_uri: Some("core://project-alpha".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("first alias should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("core://zone/release_plan".to_string()),
            target_uri: Some("core://project-alpha".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("second alias should succeed");

    let alias_view = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://alias".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("alias view should succeed");

    let view = &alias_view["result"]["view"];
    let recommendations = view["recommendations"]
        .as_array()
        .expect("recommendations should be an array");
    assert_eq!(recommendations.len(), 1);
    assert_eq!(recommendations[0]["reviewPriority"], "high");
    assert_eq!(
        recommendations[0]["priorityReason"],
        "missing triggers across 3 alias paths"
    );
    assert_eq!(
        recommendations[0]["suggestedKeywords"],
        json!(["alpha", "hub", "launch"])
    );
    assert_eq!(
        recommendations[0]["command"],
        "codex zmemory manage-triggers core://hub/launch-plan --add alpha --add hub --add launch --json"
    );

    let entries = view["entries"]
        .as_array()
        .expect("entries should be an array");
    assert_eq!(entries[0]["nodeUri"], "core://hub/launch-plan");
    assert_eq!(entries[0]["reviewPriority"], "high");
    assert_eq!(
        entries[0]["priorityReason"],
        "missing triggers across 3 alias paths"
    );
    assert_eq!(
        entries[0]["suggestedKeywords"],
        json!(["alpha", "hub", "launch"])
    );
}

#[test]
fn export_by_uri_keeps_requested_path_primary_and_includes_aliases_and_keywords() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://export-target".to_string()),
            content: Some("Exported content".to_string()),
            priority: Some(4),
            disclosure: Some("profile".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("alias://export-target-copy".to_string()),
            target_uri: Some("core://export-target".to_string()),
            priority: Some(6),
            disclosure: Some("mirror".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add alias should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://export-target".to_string()),
            add: Some(vec!["profile".to_string(), "agent".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("manage triggers should succeed");

    let export = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Export,
            uri: Some("alias://export-target-copy".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("export should succeed");

    assert_eq!(export["action"], "export");
    assert_eq!(export["result"]["scope"]["type"], "uri");
    assert_eq!(
        export["result"]["scope"]["value"],
        "alias://export-target-copy"
    );
    assert_eq!(export["result"]["count"], 1);
    assert_eq!(
        export["result"]["items"][0]["uri"],
        "alias://export-target-copy"
    );
    assert_eq!(export["result"]["items"][0]["content"], "Exported content");
    assert_eq!(export["result"]["items"][0]["priority"], 6);
    assert_eq!(export["result"]["items"][0]["disclosure"], "mirror");
    assert_eq!(
        export["result"]["items"][0]["keywords"],
        json!(["agent", "profile"])
    );
    assert_eq!(
        export["result"]["items"][0]["aliases"][0]["uri"],
        "core://export-target"
    );
}

#[test]
fn add_alias_rejects_shared_edge_metadata_conflicts() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://shared-edge".to_string()),
            content: Some("Shared edge".to_string()),
            priority: Some(1),
            disclosure: Some("primary".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");

    let alias = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("alias://shared-edge".to_string()),
            target_uri: Some("core://shared-edge".to_string()),
            priority: Some(2),
            disclosure: Some("alias".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("domain-scoped alias metadata should succeed");

    assert_eq!(alias["result"]["priority"], 1);
    assert_eq!(alias["result"]["disclosure"], "primary");
}

#[test]
fn export_by_domain_uses_domain_scoped_primary_paths() {
    let (_dir, config) = config();

    for uri in ["core://domain-one", "core://domain-two"] {
        crate::service::execute_action(
            &config,
            &ZmemoryToolCallParam {
                action: ZmemoryToolAction::Create,
                uri: Some(uri.to_string()),
                content: Some(format!("content for {uri}")),
                ..ZmemoryToolCallParam::default()
            },
        )
        .expect("create should succeed");
    }
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("alias://domain-two".to_string()),
            target_uri: Some("core://domain-two".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add alias should succeed");

    let export = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Export,
            domain: Some("core".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("domain export should succeed");

    assert_eq!(export["result"]["scope"]["type"], "domain");
    assert_eq!(export["result"]["scope"]["value"], "core");
    assert_eq!(export["result"]["count"], 2);
    let items = export["result"]["items"]
        .as_array()
        .expect("items should be array");
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|item| {
        item["uri"]
            .as_str()
            .unwrap_or_default()
            .starts_with("core://")
    }));
}

#[test]
fn import_creates_memories_aliases_and_keywords_in_one_transaction() {
    let (_dir, config) = config();

    let import = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Import,
            items: Some(vec![json!({
                "uri": "core://import-target",
                "content": "Imported content",
                "priority": 2,
                "disclosure": "profile",
                "keywords": ["profile", "agent"],
                "aliases": [
                    {
                        "uri": "alias://import-target-copy",
                        "priority": 5,
                        "disclosure": "mirror"
                    }
                ]
            })]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("import should succeed");

    assert_eq!(import["action"], "import");
    assert_eq!(import["result"]["count"], 1);
    assert_eq!(
        import["result"]["results"][0]["uri"],
        "core://import-target"
    );

    let read_primary = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://import-target".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read primary should succeed");
    assert_eq!(read_primary["result"]["content"], "Imported content");
    assert_eq!(read_primary["result"]["priority"], 2);
    assert_eq!(read_primary["result"]["disclosure"], "profile");
    assert_eq!(
        read_primary["result"]["keywords"],
        json!(["agent", "profile"])
    );
    assert_eq!(read_primary["result"]["aliasCount"], 2);

    let read_alias = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("alias://import-target-copy".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("read alias should succeed");
    assert_eq!(read_alias["result"]["content"], "Imported content");
}

#[test]
fn import_rolls_back_when_alias_path_conflicts() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://existing-target".to_string()),
            content: Some("existing".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("alias://conflict".to_string()),
            target_uri: Some("core://existing-target".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("add alias should succeed");

    let error = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Import,
            items: Some(vec![json!({
                "uri": "core://import-conflict",
                "content": "should rollback",
                "aliases": [
                    {
                        "uri": "alias://conflict"
                    }
                ]
            })]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("import should fail");
    assert_eq!(
        error.to_string(),
        "alias path already exists: alias://conflict"
    );

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://import-conflict".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    );
    assert!(read.is_err());
}

#[test]
fn import_rolls_back_when_primary_uri_conflicts() {
    let (_dir, config) = config();

    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://import-conflict".to_string()),
            content: Some("existing".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");

    let error = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Import,
            items: Some(vec![json!({
                "uri": "core://import-conflict",
                "content": "should fail"
            })]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect_err("import should fail");
    assert_eq!(
        error.to_string(),
        "memory already exists at core://import-conflict"
    );

    let read = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("core://import-conflict".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("existing memory should remain readable");
    assert_eq!(read["result"]["content"], "existing");
}

#[test]
fn alias_view_uses_real_existing_path_for_cross_domain_alias_nodes() {
    let (_dir, config) = config_with_settings(ZmemorySettings::from_env_vars(
        Some("core,writer".to_string()),
        None,
    ));
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://project-alpha".to_string()),
            content: Some("Cross-domain alias seed".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::AddAlias,
            new_uri: Some("writer://mirror-note".to_string()),
            target_uri: Some("core://project-alpha".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("cross-domain alias should succeed");

    let alias_view = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Read,
            uri: Some("system://alias".to_string()),
            limit: Some(10),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("alias view should succeed");

    let entry = &alias_view["result"]["view"]["entries"][0];
    let node_uri = entry["nodeUri"]
        .as_str()
        .expect("nodeUri should be a string");
    assert!(node_uri == "core://project-alpha" || node_uri == "writer://mirror-note");
    let command = alias_view["result"]["view"]["recommendations"][0]["command"]
        .as_str()
        .expect("command should be a string");
    assert!(command.contains(node_uri));
}

#[test]
fn doctor_reports_fts_and_keyword_inconsistencies() {
    let (_dir, config) = config();
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Create,
            uri: Some("core://salem".to_string()),
            content: Some("Profile for Salem".to_string()),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("create should succeed");
    crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::ManageTriggers,
            uri: Some("core://salem".to_string()),
            add: Some(vec!["profile".to_string()]),
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("manage triggers should succeed");

    let conn = Connection::open(config.db_path()).expect("db should open");
    conn.execute("DELETE FROM search_documents_fts", [])
        .expect("fts delete should succeed");
    conn.execute(
        "DELETE FROM paths WHERE domain = ?1 AND path = ?2",
        params!["core", "salem"],
    )
    .expect("path delete should succeed");

    let doctor = crate::service::execute_action(
        &config,
        &ZmemoryToolCallParam {
            action: ZmemoryToolAction::Doctor,
            ..ZmemoryToolCallParam::default()
        },
    )
    .expect("doctor should succeed");

    assert_eq!(doctor["result"]["healthy"], false);
    let issues = doctor["result"]["issues"]
        .as_array()
        .expect("issues should be an array");
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "fts_count_mismatch")
    );
    assert!(
        issues
            .iter()
            .any(|issue| issue["code"] == "dangling_keywords")
    );
}

fn sorted_object_keys(value: &Value) -> Vec<&str> {
    let mut keys = value
        .as_object()
        .expect("value should be an object")
        .keys()
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort_unstable();
    keys
}

use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::index;
use crate::tool_api::CreateActionParams;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;
use serde_json::json;
use uuid::Uuid;

pub(crate) fn create_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &CreateActionParams,
) -> Result<Value> {
    let tx = conn.transaction()?;
    let result = create_action_in_tx(config, &tx, args)?;
    tx.commit()?;

    let document_count = common::search_document_count(conn, config)?;
    Ok(augment_create_result(result, document_count))
}

pub(crate) fn create_action_in_tx(
    config: &ZmemoryConfig,
    conn: &rusqlite::Transaction<'_>,
    args: &CreateActionParams,
) -> Result<Value> {
    let uri = resolve_create_uri(config, conn, args)?;
    common::ensure_writable_domain(config, conn, &uri.domain)?;
    anyhow::ensure!(
        common::find_path_row(conn, config, &uri)?.is_none(),
        "memory already exists at {uri}"
    );

    let parent_uri = uri.parent();
    let parent = if parent_uri.is_root() {
        common::PathRow::root()
    } else {
        common::find_path_row(conn, config, &parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?
    };
    let node_uuid = Uuid::new_v4().to_string();
    let priority = args.priority;
    let disclosure = common::normalize_optional_text(args.disclosure.as_deref());

    conn.execute("INSERT INTO nodes(uuid) VALUES (?1)", [node_uuid.as_str()])?;
    conn.execute(
        "INSERT INTO memories(namespace, node_uuid, content) VALUES (?1, ?2, ?3)",
        params![config.namespace(), node_uuid, args.content],
    )?;
    let memory_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO edges(namespace, parent_uuid, child_uuid, name, priority, disclosure)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            config.namespace(),
            parent.node_uuid,
            node_uuid,
            uri.leaf_name()?,
            priority,
            disclosure
        ],
    )?;
    let edge_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO paths(namespace, domain, path, edge_id) VALUES (?1, ?2, ?3, ?4)",
        params![config.namespace(), uri.domain, uri.path, edge_id],
    )?;
    common::insert_audit_log(
        conn,
        config.namespace(),
        "create",
        Some(&uri.to_string()),
        Some(&node_uuid),
        json!({
            "memoryId": memory_id,
            "priority": priority,
            "disclosure": disclosure,
        }),
    )?;
    index::reindex_node(conn, config.namespace(), &node_uuid)?;

    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": node_uuid,
        "memoryId": memory_id,
        "priority": priority,
        "disclosure": disclosure,
    }))
}

fn augment_create_result(mut result: Value, document_count: i64) -> Value {
    result["documentCount"] = json!(document_count);
    result
}

fn resolve_create_uri(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &CreateActionParams,
) -> Result<ZmemoryUri> {
    anyhow::ensure!(
        !(args.uri.is_some() && (args.parent_uri.is_some() || args.title.is_some())),
        "`uri` cannot be combined with `parentUri` or `title`",
    );

    if let Some(uri) = args.uri.as_ref() {
        let uri = uri.clone();
        anyhow::ensure!(!uri.is_root(), "cannot create root path");
        return Ok(uri);
    }

    let parent_uri = parse_parent_uri(args.parent_uri.as_ref())?;
    let title = args.title.clone();
    let name = match title {
        Some(title) => {
            validate_title(&title)?;
            title
        }
        None => next_auto_child_name(config, conn, &parent_uri)?,
    };
    let path = if parent_uri.path.is_empty() {
        name
    } else {
        format!("{}/{name}", parent_uri.path)
    };
    Ok(ZmemoryUri {
        domain: parent_uri.domain,
        path,
    })
}

fn parse_parent_uri(raw: Option<&ZmemoryUri>) -> Result<ZmemoryUri> {
    raw.cloned()
        .ok_or_else(|| anyhow::anyhow!("`uri` or `parentUri` is required"))
}

fn validate_title(title: &str) -> Result<()> {
    anyhow::ensure!(
        title
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'),
        "`title` may only contain ASCII letters, numbers, `_`, or `-`",
    );
    Ok(())
}

fn next_auto_child_name(
    config: &ZmemoryConfig,
    conn: &Connection,
    parent_uri: &ZmemoryUri,
) -> Result<String> {
    if !parent_uri.is_root() {
        common::find_path_row(conn, config, parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?;
    }

    let mut next_index = 1_u64;
    loop {
        let child_path = if parent_uri.path.is_empty() {
            next_index.to_string()
        } else {
            format!("{}/{next_index}", parent_uri.path)
        };
        let candidate = ZmemoryUri {
            domain: parent_uri.domain.clone(),
            path: child_path,
        };
        if common::find_path_row(conn, config, &candidate)?.is_none() {
            return Ok(next_index.to_string());
        }
        next_index += 1;
    }
}

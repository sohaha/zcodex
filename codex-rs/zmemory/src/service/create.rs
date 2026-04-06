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
    let uri = resolve_create_uri(conn, args)?;
    common::ensure_writable_domain(config, conn, &uri.domain)?;
    anyhow::ensure!(
        common::find_path_row(conn, &uri)?.is_none(),
        "memory already exists at {uri}"
    );

    let parent_uri = uri.parent();
    let parent = if parent_uri.is_root() {
        common::PathRow::root()
    } else {
        common::find_path_row(conn, &parent_uri)?
            .ok_or_else(|| anyhow::anyhow!("parent path does not exist: {parent_uri}"))?
    };
    let node_uuid = Uuid::new_v4().to_string();
    let priority = args.priority;
    let disclosure = common::normalize_optional_text(args.disclosure.as_deref());

    let tx = conn.transaction()?;
    tx.execute("INSERT INTO nodes(uuid) VALUES (?1)", [node_uuid.as_str()])?;
    tx.execute(
        "INSERT INTO memories(node_uuid, content) VALUES (?1, ?2)",
        params![node_uuid, args.content],
    )?;
    let memory_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO edges(parent_uuid, child_uuid, name, priority, disclosure) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![parent.node_uuid, node_uuid, uri.leaf_name()?, priority, disclosure],
    )?;
    let edge_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO paths(domain, path, edge_id) VALUES (?1, ?2, ?3)",
        params![uri.domain, uri.path, edge_id],
    )?;
    common::insert_audit_log(
        &tx,
        "create",
        Some(&uri.to_string()),
        Some(&node_uuid),
        json!({
            "memoryId": memory_id,
            "priority": priority,
            "disclosure": disclosure,
        }),
    )?;
    index::reindex_node(&tx, &node_uuid)?;
    tx.commit()?;

    let document_count = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(json!({
        "uri": uri.to_string(),
        "nodeUuid": node_uuid,
        "memoryId": memory_id,
        "priority": priority,
        "disclosure": disclosure,
        "documentCount": document_count,
    }))
}

fn resolve_create_uri(conn: &Connection, args: &CreateActionParams) -> Result<ZmemoryUri> {
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
        None => next_auto_child_name(conn, &parent_uri)?,
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

fn next_auto_child_name(conn: &Connection, parent_uri: &ZmemoryUri) -> Result<String> {
    if !parent_uri.is_root() {
        common::find_path_row(conn, parent_uri)?
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
        if common::find_path_row(conn, &candidate)?.is_none() {
            return Ok(next_index.to_string());
        }
        next_index += 1;
    }
}

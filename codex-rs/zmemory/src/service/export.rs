use crate::config::ZmemoryConfig;
use crate::service::common;
use crate::service::contracts::ExportNodeContract;
use crate::service::contracts::ExportResultContract;
use crate::service::contracts::ExportScopeContract;
use crate::service::snapshot;
use crate::tool_api::ExportActionParams;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;
use rusqlite::Connection;
use rusqlite::params;
use serde_json::Value;

pub(crate) fn export_action(
    config: &ZmemoryConfig,
    conn: &Connection,
    args: &ExportActionParams,
) -> Result<Value> {
    let result = match (&args.uri, &args.domain) {
        (Some(uri), None) => export_uri_scope(config, conn, uri)?,
        (None, Some(domain)) => export_domain_scope(config, conn, domain)?,
        _ => anyhow::bail!("exactly one of `uri` or `domain` is required for action=export"),
    };

    serde_json::to_value(result).map_err(Into::into)
}

fn export_uri_scope(
    config: &ZmemoryConfig,
    conn: &Connection,
    uri: &ZmemoryUri,
) -> Result<ExportResultContract> {
    common::ensure_readable_domain(config, conn, &uri.domain)?;
    let row = common::find_path_row(conn, config, uri)?
        .ok_or_else(|| anyhow::anyhow!("memory not found: {uri}"))?;
    let item = export_item(config, conn, &row.node_uuid, Some(uri), None)?;
    Ok(ExportResultContract {
        scope: ExportScopeContract {
            r#type: "uri".to_string(),
            value: uri.to_string(),
        },
        count: 1,
        items: vec![item],
    })
}

fn export_domain_scope(
    config: &ZmemoryConfig,
    conn: &Connection,
    domain: &str,
) -> Result<ExportResultContract> {
    common::ensure_readable_domain(config, conn, domain)?;
    let mut stmt = conn.prepare(
        "SELECT DISTINCT e.child_uuid
         FROM paths p
         JOIN edges e ON e.id = p.edge_id AND e.namespace = p.namespace
         WHERE p.namespace = ?1 AND p.domain = ?2
         ORDER BY e.child_uuid ASC",
    )?;
    let node_uuids = stmt
        .query_map(params![config.namespace(), domain], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let items = node_uuids
        .iter()
        .map(|node_uuid| export_item(config, conn, node_uuid, None, Some(domain)))
        .collect::<Result<Vec<_>>>()?;

    Ok(ExportResultContract {
        scope: ExportScopeContract {
            r#type: "domain".to_string(),
            value: domain.to_string(),
        },
        count: items.len(),
        items,
    })
}

fn export_item(
    config: &ZmemoryConfig,
    conn: &Connection,
    node_uuid: &str,
    requested_uri: Option<&ZmemoryUri>,
    preferred_domain: Option<&str>,
) -> Result<ExportNodeContract> {
    let snapshot = snapshot::load_node_snapshot_for_node(
        config,
        conn,
        node_uuid,
        requested_uri,
        preferred_domain,
    )?;
    Ok(ExportNodeContract {
        uri: snapshot.primary_uri,
        content: snapshot.content,
        priority: snapshot.priority,
        disclosure: snapshot.disclosure,
        keywords: snapshot.keywords,
        aliases: snapshot.aliases,
    })
}

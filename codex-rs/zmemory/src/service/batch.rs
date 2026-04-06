use crate::config::ZmemoryConfig;
use crate::service::create::create_action_in_tx;
use crate::service::update::update_action_in_tx;
use crate::tool_api::BatchCreateActionParams;
use crate::tool_api::BatchUpdateActionParams;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub(crate) fn batch_create_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &BatchCreateActionParams,
) -> Result<Value> {
    let tx = conn.transaction()?;
    let results = args
        .items
        .iter()
        .map(|item| create_action_in_tx(config, &tx, item))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;

    let document_count = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(json!({
        "count": results.len(),
        "results": results,
        "documentCount": document_count,
    }))
}

pub(crate) fn batch_update_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &BatchUpdateActionParams,
) -> Result<Value> {
    let tx = conn.transaction()?;
    let results = args
        .items
        .iter()
        .map(|item| update_action_in_tx(config, &tx, item))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;

    let document_count = conn.query_row("SELECT COUNT(*) FROM search_documents", [], |row| {
        row.get::<_, i64>(0)
    })?;
    Ok(json!({
        "count": results.len(),
        "results": results,
        "documentCount": document_count,
    }))
}

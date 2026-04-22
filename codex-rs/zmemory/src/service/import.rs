use crate::config::ZmemoryConfig;
use crate::service::alias;
use crate::service::create;
use crate::tool_api::AddAliasActionParams;
use crate::tool_api::CreateActionParams;
use crate::tool_api::ImportActionParams;
use crate::tool_api::ImportItemActionParams;
use crate::tool_api::ManageTriggersActionParams;
use anyhow::Result;
use rusqlite::Connection;
use serde_json::Value;
use serde_json::json;

pub(crate) fn import_action(
    config: &ZmemoryConfig,
    conn: &mut Connection,
    args: &ImportActionParams,
) -> Result<Value> {
    let tx = conn.transaction()?;
    let results = args
        .items
        .iter()
        .map(|item| import_item_in_tx(config, &tx, item))
        .collect::<Result<Vec<_>>>()?;
    tx.commit()?;

    let document_count = crate::service::common::search_document_count(conn, config)?;
    Ok(json!({
        "count": results.len(),
        "results": results,
        "documentCount": document_count,
    }))
}

fn import_item_in_tx(
    config: &ZmemoryConfig,
    conn: &rusqlite::Transaction<'_>,
    item: &ImportItemActionParams,
) -> Result<Value> {
    let create_result = create::create_action_in_tx(
        config,
        conn,
        &CreateActionParams {
            uri: Some(item.uri.clone()),
            parent_uri: None,
            content: item.content.clone(),
            title: None,
            priority: item.priority,
            disclosure: item.disclosure.clone(),
        },
    )?;

    for alias_item in &item.aliases {
        alias::add_alias_action_in_tx(
            config,
            conn,
            &AddAliasActionParams {
                new_uri: alias_item.uri.clone(),
                target_uri: item.uri.clone(),
                priority: alias_item.priority,
                disclosure: alias_item.disclosure.clone(),
            },
        )?;
    }

    if !item.keywords.is_empty() {
        alias::manage_triggers_action_in_tx(
            config,
            conn,
            &ManageTriggersActionParams {
                uri: item.uri.clone(),
                add: item.keywords.clone(),
                remove: Vec::new(),
            },
        )?;
    }

    Ok(json!({
        "uri": item.uri.to_string(),
        "nodeUuid": create_result["nodeUuid"],
        "aliasCount": item.aliases.len(),
        "keywordCount": item.keywords.len(),
        "governance": create_result["governance"].clone(),
    }))
}

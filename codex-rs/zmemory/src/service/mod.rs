//! Action dispatch and re-exports for the zmemory service layer.

mod alias;
mod common;
mod create;
mod delete;
pub(crate) mod index;
mod read;
mod search;
mod stats;
#[cfg(test)]
mod tests;
mod update;

pub(crate) use alias::add_alias_action;
pub(crate) use alias::manage_triggers_action;
pub(crate) use common::stats_queries;
pub(crate) use read::read_action;
pub(crate) use search::search_action;
pub(crate) use stats::doctor_action;
pub(crate) use stats::rebuild_search_action;
pub(crate) use stats::stats_action;

use crate::config::ZmemoryConfig;
use crate::repository::ZmemoryRepository;
use crate::tool_api::ZmemoryToolAction;
use crate::tool_api::ZmemoryToolCallParam;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;

pub(crate) fn execute_action(config: &ZmemoryConfig, args: &ZmemoryToolCallParam) -> Result<Value> {
    let repository = ZmemoryRepository::new(config.clone());
    let mut conn = repository.connect()?;
    let result = match args.action {
        ZmemoryToolAction::Read => read_action(config, &conn, args)?,
        ZmemoryToolAction::Search => search_action(config, &conn, args)?,
        ZmemoryToolAction::Create => create::create_action(config, &mut conn, args)?,
        ZmemoryToolAction::Update => update::update_action(config, &mut conn, args)?,
        ZmemoryToolAction::DeletePath => delete::delete_path_action(config, &mut conn, args)?,
        ZmemoryToolAction::AddAlias => add_alias_action(config, &mut conn, args)?,
        ZmemoryToolAction::ManageTriggers => manage_triggers_action(config, &mut conn, args)?,
        ZmemoryToolAction::Stats => stats_action(&conn, config)?,
        ZmemoryToolAction::Doctor => doctor_action(&conn, config)?,
        ZmemoryToolAction::RebuildSearch => rebuild_search_action(&mut conn)?,
    };
    Ok(json!({
        "action": action_name(args.action.clone()),
        "result": result,
    }))
}

fn action_name(action: ZmemoryToolAction) -> &'static str {
    match action {
        ZmemoryToolAction::Read => "read",
        ZmemoryToolAction::Search => "search",
        ZmemoryToolAction::Create => "create",
        ZmemoryToolAction::Update => "update",
        ZmemoryToolAction::DeletePath => "delete-path",
        ZmemoryToolAction::AddAlias => "add-alias",
        ZmemoryToolAction::ManageTriggers => "manage-triggers",
        ZmemoryToolAction::Stats => "stats",
        ZmemoryToolAction::Doctor => "doctor",
        ZmemoryToolAction::RebuildSearch => "rebuild-search",
    }
}

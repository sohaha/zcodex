pub(crate) mod alias;
pub(crate) mod common;
pub(crate) mod create;
pub(crate) mod delete;
pub(crate) mod index;
pub(crate) mod read;
pub(crate) mod search;
pub(crate) mod stats;
pub(crate) mod update;

#[cfg(test)]
mod tests;

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
        ZmemoryToolAction::Read => read::read_action(config, &conn, args)?,
        ZmemoryToolAction::Search => search::search_action(config, &conn, args)?,
        ZmemoryToolAction::Create => create::create_action(config, &mut conn, args)?,
        ZmemoryToolAction::Update => update::update_action(config, &mut conn, args)?,
        ZmemoryToolAction::DeletePath => delete::delete_path_action(config, &mut conn, args)?,
        ZmemoryToolAction::AddAlias => alias::add_alias_action(config, &mut conn, args)?,
        ZmemoryToolAction::ManageTriggers => {
            alias::manage_triggers_action(config, &mut conn, args)?
        }
        ZmemoryToolAction::Stats => stats::stats_action(&conn, config)?,
        ZmemoryToolAction::Doctor => stats::doctor_action(&conn, config)?,
        ZmemoryToolAction::RebuildSearch => stats::rebuild_search_action(&mut conn)?,
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

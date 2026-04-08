use crate::config::ZmemoryConfig;
use crate::tool_api::ZmemoryActionInput;
use crate::tool_api::ZmemoryToolCallParam;
use anyhow::Result;
use serde_json::Value;
use serde_json::json;

mod alias;
mod batch;
mod common;
pub(crate) mod contracts;
mod create;
mod delete;
mod export;
mod history;
mod import;
mod index;
mod read;
pub(crate) mod review;
mod search;
mod snapshot;
pub(crate) mod stats;
mod update;

#[cfg(test)]
mod tests;

pub(crate) fn execute_action(config: &ZmemoryConfig, args: &ZmemoryToolCallParam) -> Result<Value> {
    let mut conn = common::connect(config)?;
    let typed = ZmemoryActionInput::try_from(args)?;
    let result = match &typed {
        ZmemoryActionInput::Read(params) => read::read_action(config, &conn, params)?,
        ZmemoryActionInput::History(params) => history::history_action(config, &conn, &params.uri)?,
        ZmemoryActionInput::Search(params) => search::search_action(config, &conn, params)?,
        ZmemoryActionInput::Export(params) => export::export_action(config, &conn, params)?,
        ZmemoryActionInput::Import(params) => import::import_action(config, &mut conn, params)?,
        ZmemoryActionInput::Create(params) => create::create_action(config, &mut conn, params)?,
        ZmemoryActionInput::BatchCreate(params) => {
            batch::batch_create_action(config, &mut conn, params)?
        }
        ZmemoryActionInput::Update(params) => update::update_action(config, &mut conn, params)?,
        ZmemoryActionInput::BatchUpdate(params) => {
            batch::batch_update_action(config, &mut conn, params)?
        }
        ZmemoryActionInput::DeletePath(params) => {
            delete::delete_path_action(config, &mut conn, params)?
        }
        ZmemoryActionInput::AddAlias(params) => alias::add_alias_action(config, &mut conn, params)?,
        ZmemoryActionInput::ManageTriggers(params) => {
            alias::manage_triggers_action(config, &mut conn, params)?
        }
        ZmemoryActionInput::Stats => stats::stats_action(&conn, config)?,
        ZmemoryActionInput::Audit(params) => stats::audit_action(&conn, config, params)?,
        ZmemoryActionInput::Doctor => stats::doctor_action(&conn, config)?,
        ZmemoryActionInput::RebuildSearch => stats::rebuild_search_action(&mut conn, config)?,
    };
    Ok(json!({
        "action": action_name(&typed),
        "result": result,
    }))
}

fn action_name(action: &ZmemoryActionInput) -> &'static str {
    match action {
        ZmemoryActionInput::Read(_) => "read",
        ZmemoryActionInput::History(_) => "history",
        ZmemoryActionInput::Search(_) => "search",
        ZmemoryActionInput::Export(_) => "export",
        ZmemoryActionInput::Import(_) => "import",
        ZmemoryActionInput::Create(_) => "create",
        ZmemoryActionInput::BatchCreate(_) => "batch-create",
        ZmemoryActionInput::Update(_) => "update",
        ZmemoryActionInput::BatchUpdate(_) => "batch-update",
        ZmemoryActionInput::DeletePath(_) => "delete-path",
        ZmemoryActionInput::AddAlias(_) => "add-alias",
        ZmemoryActionInput::ManageTriggers(_) => "manage-triggers",
        ZmemoryActionInput::Stats => "stats",
        ZmemoryActionInput::Audit(_) => "audit",
        ZmemoryActionInput::Doctor => "doctor",
        ZmemoryActionInput::RebuildSearch => "rebuild-search",
    }
}

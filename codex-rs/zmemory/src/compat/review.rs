use super::CompatService;
use super::contracts::GlossaryChangeResponse;
use super::contracts::PathChangeResponse;
use super::contracts::ReviewDiffResponse;
use super::contracts::ReviewGroupItemResponse;
use super::contracts::StateMetaResponse;
use crate::service::contracts::ReviewGroupDiffContract;
use anyhow::Result;
use serde_json::Value;

impl CompatService {
    pub fn review_groups(&self, namespace: Option<&str>) -> Result<Vec<ReviewGroupItemResponse>> {
        let (conn, config) = self.connect(namespace)?;
        crate::service::review::review_groups(&conn, &config, 200)?
            .into_iter()
            .map(|group| {
                let history_len = crate::service::history::history_versions_for_node(
                    &conn,
                    config.namespace(),
                    &group.node_uuid,
                )?
                .len() as i64;
                let top_level_table = if group.missing_triggers {
                    "glossary_keywords"
                } else if history_len > 1 {
                    "memories"
                } else {
                    "paths"
                };
                let namespaces = if config.namespace().is_empty() {
                    None
                } else {
                    Some(vec![config.namespace().to_string()])
                };
                Ok(ReviewGroupItemResponse {
                    node_uuid: group.node_uuid,
                    display_uri: group.node_uri,
                    top_level_table: top_level_table.to_string(),
                    action: "modified".to_string(),
                    row_count: (group.alias_count + group.trigger_count).max(1),
                    namespaces,
                })
            })
            .collect()
    }

    pub fn review_group_diff(
        &self,
        namespace: Option<&str>,
        node_uuid: &str,
    ) -> Result<ReviewDiffResponse> {
        let (conn, config) = self.connect(namespace)?;
        let diff =
            crate::service::review::review_group_diff_for_node_uuid(&conn, &config, node_uuid)?;
        Ok(review_diff_response(diff))
    }
}

fn review_diff_response(diff: ReviewGroupDiffContract) -> ReviewDiffResponse {
    let before_content = diff
        .changeset
        .versions
        .iter()
        .find(|version| version.id != diff.snapshot.memory_id)
        .map(|version| version.content.clone());
    let current_meta = StateMetaResponse {
        priority: Some(diff.snapshot.priority),
        disclosure: diff.snapshot.disclosure.clone(),
    };
    let before_meta = current_meta.clone();
    let path_changes = path_changes(&diff.recent_audit_entries);
    let glossary_changes = glossary_changes(&diff.recent_audit_entries);
    let active_paths = std::iter::once(diff.snapshot.uri.clone())
        .chain(diff.snapshot.aliases.iter().map(|alias| alias.uri.clone()))
        .collect::<Vec<_>>();
    ReviewDiffResponse {
        uri: diff.snapshot.uri,
        change_type: if !glossary_changes.is_empty() {
            "glossary_keywords".to_string()
        } else if before_content.is_some() {
            "memories".to_string()
        } else {
            "paths".to_string()
        },
        action: "modified".to_string(),
        before_content,
        current_content: Some(diff.snapshot.content),
        before_meta,
        current_meta,
        path_changes,
        glossary_changes,
        active_paths,
        has_changes: true,
    }
}

fn path_changes(
    entries: &[crate::service::contracts::AuditEntryContract],
) -> Vec<PathChangeResponse> {
    let mut changes = Vec::new();
    for entry in entries {
        match entry.action.as_str() {
            "add-alias" | "create" => {
                if let Some(uri) = entry.uri.as_ref().filter(|uri| !uri.is_empty()) {
                    changes.push(PathChangeResponse {
                        action: "created".to_string(),
                        uri: uri.clone(),
                        namespace: String::new(),
                    });
                }
            }
            "delete-path" => {
                if let Some(uri) = entry.uri.as_ref().filter(|uri| !uri.is_empty()) {
                    changes.push(PathChangeResponse {
                        action: "deleted".to_string(),
                        uri: uri.clone(),
                        namespace: String::new(),
                    });
                }
            }
            _ => {}
        }
    }
    changes
}

fn glossary_changes(
    entries: &[crate::service::contracts::AuditEntryContract],
) -> Vec<GlossaryChangeResponse> {
    let mut changes = Vec::new();
    for entry in entries {
        if entry.action != "manage-triggers" {
            continue;
        }
        let Some(object) = entry.details.as_object() else {
            continue;
        };
        if let Some(added) = object.get("added").and_then(Value::as_array) {
            for keyword in added.iter().filter_map(Value::as_str) {
                changes.push(GlossaryChangeResponse {
                    action: "created".to_string(),
                    keyword: keyword.to_string(),
                });
            }
        }
        if let Some(removed) = object.get("removed").and_then(Value::as_array) {
            for keyword in removed.iter().filter_map(Value::as_str) {
                changes.push(GlossaryChangeResponse {
                    action: "deleted".to_string(),
                    keyword: keyword.to_string(),
                });
            }
        }
    }
    changes
}

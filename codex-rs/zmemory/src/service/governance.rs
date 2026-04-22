use crate::service::contracts::ContentGovernanceIssueContract;
use crate::service::contracts::ContentGovernanceResultContract;
use crate::service::contracts::ContentGovernanceRuleContract;
use crate::service::contracts::ContentGovernanceScopeContract;
use crate::tool_api::ZmemoryUri;
use anyhow::Result;

const COLLABORATION_CONTRACT_HEADER: &str = "Shared collaboration contract:";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContentGovernanceScope {
    AssistantSelfReference,
    UserAddressPreference,
    CollaborationContract,
}

impl ContentGovernanceScope {
    const fn kind(self) -> &'static str {
        match self {
            Self::AssistantSelfReference => "assistantSelfReference",
            Self::UserAddressPreference => "userAddressPreference",
            Self::CollaborationContract => "collaborationContract",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleOutcome {
    Accepted,
    Normalized,
    Conflict,
}

impl RuleOutcome {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Normalized => "normalized",
            Self::Conflict => "conflict",
        }
    }
}

#[derive(Debug, Clone)]
struct RuleEvaluation {
    outcome: RuleOutcome,
    governed_content: String,
    issues: Vec<ContentGovernanceIssueContract>,
    message: String,
}

impl RuleEvaluation {
    fn accepted(content: &str, message: impl Into<String>) -> Self {
        Self {
            outcome: RuleOutcome::Accepted,
            governed_content: content.to_string(),
            issues: Vec::new(),
            message: message.into(),
        }
    }

    fn normalized(content: String, message: impl Into<String>) -> Self {
        Self {
            outcome: RuleOutcome::Normalized,
            governed_content: content,
            issues: Vec::new(),
            message: message.into(),
        }
    }

    fn conflict(content: &str, code: &str, message: impl Into<String>) -> Self {
        let message = message.into();
        Self {
            outcome: RuleOutcome::Conflict,
            governed_content: content.to_string(),
            issues: vec![ContentGovernanceIssueContract {
                code: code.to_string(),
                severity: "error".to_string(),
                message: message.clone(),
            }],
            message,
        }
    }
}

struct ContentGovernanceRule {
    id: &'static str,
    scope: ContentGovernanceScope,
    evaluate: fn(&str) -> RuleEvaluation,
}

const CONTENT_GOVERNANCE_RULES: &[ContentGovernanceRule] = &[
    ContentGovernanceRule {
        id: "canonical-agent-self-reference",
        scope: ContentGovernanceScope::AssistantSelfReference,
        evaluate: govern_agent_self_reference,
    },
    ContentGovernanceRule {
        id: "canonical-user-address-preference",
        scope: ContentGovernanceScope::UserAddressPreference,
        evaluate: govern_user_address_preference,
    },
    ContentGovernanceRule {
        id: "canonical-collaboration-contract",
        scope: ContentGovernanceScope::CollaborationContract,
        evaluate: govern_collaboration_contract,
    },
];

pub(crate) fn evaluate_content(uri: &ZmemoryUri, content: &str) -> ContentGovernanceResultContract {
    let Some(scope) = governance_scope_for_uri(uri) else {
        return ContentGovernanceResultContract {
            status: "notApplicable".to_string(),
            scope: None,
            changed: false,
            governed_content: content.to_string(),
            issues: Vec::new(),
            rules: Vec::new(),
        };
    };

    let mut governed_content = content.to_string();
    let mut status = "accepted".to_string();
    let mut issues = Vec::new();
    let mut rules = Vec::new();

    for rule in CONTENT_GOVERNANCE_RULES
        .iter()
        .filter(|rule| rule.scope == scope)
    {
        let evaluation = (rule.evaluate)(&governed_content);
        if matches!(evaluation.outcome, RuleOutcome::Normalized) {
            status = "normalized".to_string();
        }
        if matches!(evaluation.outcome, RuleOutcome::Conflict) {
            status = "conflict".to_string();
            issues.extend(evaluation.issues.clone());
        }
        governed_content = evaluation.governed_content;
        rules.push(ContentGovernanceRuleContract {
            rule_id: rule.id.to_string(),
            outcome: evaluation.outcome.as_str().to_string(),
            message: evaluation.message,
        });
        if matches!(evaluation.outcome, RuleOutcome::Conflict) {
            break;
        }
    }

    ContentGovernanceResultContract {
        status,
        scope: Some(ContentGovernanceScopeContract {
            uri: uri.to_string(),
            kind: scope.kind().to_string(),
        }),
        changed: governed_content != content,
        governed_content,
        issues,
        rules,
    }
}

impl ContentGovernanceResultContract {
    pub(crate) fn has_conflicts(&self) -> bool {
        self.status == "conflict"
    }

    pub(crate) fn conflict_summary(&self) -> Option<String> {
        (!self.issues.is_empty()).then(|| {
            self.issues
                .iter()
                .map(|issue| issue.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        })
    }
}

pub(crate) fn evaluate_write_content(
    uri: &ZmemoryUri,
    content: &str,
) -> Result<ContentGovernanceResultContract> {
    let result = evaluate_content(uri, content);
    anyhow::ensure!(
        !result.has_conflicts(),
        "{}",
        result
            .conflict_summary()
            .unwrap_or_else(|| format!("content governance rejected write for {uri}"))
    );
    Ok(result)
}

fn governance_scope_for_uri(uri: &ZmemoryUri) -> Option<ContentGovernanceScope> {
    match (uri.domain.as_str(), uri.path.as_str()) {
        ("core", "agent") => Some(ContentGovernanceScope::AssistantSelfReference),
        ("core", "my_user") => Some(ContentGovernanceScope::UserAddressPreference),
        ("core", "agent/my_user") => Some(ContentGovernanceScope::CollaborationContract),
        _ => None,
    }
}

fn govern_agent_self_reference(content: &str) -> RuleEvaluation {
    govern_name_statement(
        content,
        "The assistant should refer to itself as",
        "assistant_self_reference_conflict",
        "assistant self-reference",
    )
}

fn govern_user_address_preference(content: &str) -> RuleEvaluation {
    govern_name_statement(
        content,
        "The user prefers to be addressed as",
        "user_address_preference_conflict",
        "user address preference",
    )
}

fn govern_name_statement(
    content: &str,
    template_prefix: &str,
    conflict_code: &str,
    label: &str,
) -> RuleEvaluation {
    let names = dedup_preserve_order(extract_quoted_values(content));
    match names.as_slice() {
        [] => RuleEvaluation::accepted(content, format!("no explicit {label} value detected")),
        [name] => {
            let normalized = format!("{template_prefix} \"{name}\".");
            if content.trim() == normalized {
                RuleEvaluation::accepted(content, format!("{label} already canonical"))
            } else {
                RuleEvaluation::normalized(normalized, format!("normalized {label}"))
            }
        }
        _ => RuleEvaluation::conflict(
            content,
            conflict_code,
            format!(
                "found multiple distinct {label} values: {}",
                names.join(", ")
            ),
        ),
    }
}

fn govern_collaboration_contract(content: &str) -> RuleEvaluation {
    let clauses = dedup_preserve_order(
        extract_contract_clauses(content)
            .into_iter()
            .map(|clause| canonicalize_contract_clause(&clause))
            .collect(),
    );
    if clauses.is_empty() {
        return RuleEvaluation::accepted(content, "no structured collaboration clauses detected");
    }
    if let Some(message) = detect_contract_conflict(&clauses) {
        return RuleEvaluation::conflict(content, "collaboration_contract_conflict", message);
    }

    let normalized = format_contract_clauses(&clauses);
    if content.trim() == normalized {
        RuleEvaluation::accepted(content, "collaboration contract already canonical")
    } else {
        RuleEvaluation::normalized(
            normalized,
            format!("normalized {} collaboration clauses", clauses.len()),
        )
    }
}

fn extract_quoted_values(content: &str) -> Vec<String> {
    const QUOTE_PAIRS: &[(char, char)] = &[('"', '"'), ('“', '”'), ('「', '」'), ('『', '』')];

    let mut values = Vec::new();
    let mut active_quote = None;
    let mut buffer = String::new();

    for ch in content.chars() {
        if let Some(expected_end) = active_quote {
            if ch == expected_end {
                let value = buffer.trim();
                if !value.is_empty() {
                    values.push(value.to_string());
                }
                buffer.clear();
                active_quote = None;
            } else {
                buffer.push(ch);
            }
            continue;
        }

        if let Some((_, end)) = QUOTE_PAIRS.iter().find(|(start, _)| *start == ch) {
            active_quote = Some(*end);
            buffer.clear();
        }
    }

    values
}

fn extract_contract_clauses(content: &str) -> Vec<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    if let Some(rest) = trimmed.strip_prefix(COLLABORATION_CONTRACT_HEADER) {
        return rest
            .lines()
            .map(str::trim)
            .filter_map(|line| line.strip_prefix("- "))
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
    }

    let mut clauses = Vec::new();
    for line in trimmed.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(bullet) = line.strip_prefix("- ") {
            let bullet = bullet.trim();
            if !bullet.is_empty() {
                clauses.push(bullet.to_string());
            }
            continue;
        }
        clauses.extend(
            line.split(['.', '。'])
                .map(str::trim)
                .filter(|segment| !segment.is_empty())
                .map(str::to_string),
        );
    }
    clauses
}

fn canonicalize_contract_clause(clause: &str) -> String {
    let normalized_key = normalize_text_key(clause);
    match normalized_key.as_str() {
        "respond in chinese by default" => "Respond in Chinese by default.".to_string(),
        "respond in english by default" => "Respond in English by default.".to_string(),
        "keep responses concise by default" => "Keep responses concise by default.".to_string(),
        "use verbose responses by default" => "Use verbose responses by default.".to_string(),
        _ => canonicalize_naming_clause(clause)
            .unwrap_or_else(|| ensure_sentence_period(clause.trim())),
    }
}

fn canonicalize_naming_clause(clause: &str) -> Option<String> {
    let values = extract_quoted_values(clause);
    let lowercase = clause.to_lowercase();
    if values.len() == 2
        && lowercase.contains("for the assistant")
        && lowercase.contains("for the user")
    {
        return Some(format!(
            "Use \"{}\" for the assistant and \"{}\" for the user in future interactions.",
            values[0], values[1]
        ));
    }
    None
}

fn detect_contract_conflict(clauses: &[String]) -> Option<String> {
    let mut language_preference = None::<String>;
    let mut response_length = None::<String>;
    let mut naming_contract = None::<String>;

    for clause in clauses {
        let normalized_key = normalize_text_key(clause);
        let slot = if matches!(
            normalized_key.as_str(),
            "respond in chinese by default" | "respond in english by default"
        ) {
            &mut language_preference
        } else if matches!(
            normalized_key.as_str(),
            "keep responses concise by default" | "use verbose responses by default"
        ) {
            &mut response_length
        } else if canonicalize_naming_clause(clause).is_some() {
            &mut naming_contract
        } else {
            continue;
        };

        if let Some(existing) = slot {
            if existing != clause {
                return Some(format!(
                    "conflicting collaboration clauses detected for the same topic: {existing} / {clause}"
                ));
            }
        } else {
            *slot = Some(clause.clone());
        }
    }

    None
}

fn format_contract_clauses(clauses: &[String]) -> String {
    format!(
        "{COLLABORATION_CONTRACT_HEADER}\n{}",
        clauses
            .iter()
            .map(|clause| format!("- {clause}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

fn ensure_sentence_period(clause: &str) -> String {
    let clause = clause.trim().trim_end_matches(['.', '。']);
    format!("{clause}.")
}

fn normalize_text_key(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(['.', '。'])
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn dedup_preserve_order(values: Vec<String>) -> Vec<String> {
    let mut deduped = Vec::new();
    for value in values {
        if !deduped.contains(&value) {
            deduped.push(value);
        }
    }
    deduped
}

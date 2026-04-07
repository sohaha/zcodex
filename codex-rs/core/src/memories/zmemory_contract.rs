#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StablePreferenceMemory {
    UserAddressPreference,
    AgentSelfReference,
    CollaborationAddressContract,
}

impl StablePreferenceMemory {
    pub(crate) const fn uri(self) -> &'static str {
        match self {
            Self::UserAddressPreference => "core://my_user",
            Self::AgentSelfReference => "core://agent",
            Self::CollaborationAddressContract => "core://agent/my_user",
        }
    }

    pub(crate) const fn topic(self) -> &'static str {
        match self {
            Self::UserAddressPreference => "the user's preferred form of address",
            Self::AgentSelfReference => "the assistant's preferred self-name",
            Self::CollaborationAddressContract => {
                "the shared naming and addressing contract between user and assistant"
            }
        }
    }
}

pub(crate) fn stable_preference_contract_markdown() -> String {
    let mappings = [
        StablePreferenceMemory::UserAddressPreference,
        StablePreferenceMemory::AgentSelfReference,
        StablePreferenceMemory::CollaborationAddressContract,
    ]
    .into_iter()
    .map(|memory| format!("  - `{}` stores {}", memory.uri(), memory.topic()))
    .collect::<Vec<_>>()
    .join("\n");

    format!(
        "- Treat explicit, durable naming or addressing preferences as long-term memory.\n\
         - Use these canonical URIs for high-confidence preference writes:\n\
         {mappings}\n\
         - Treat those three canonical URIs as the stable identity layer for automatic recall and refinement.\n\
         - These canonical identity memories are independent from the runtime boot anchor list; use `system://workspace` to confirm the active boot profile before assuming which URIs were preloaded.\n\
         - If the input is clearly about assistant identity, user preference, or the collaboration contract, read the matching canonical URI before answering and do not ask the user which path to use.\n\
         - Before writing, inspect the current runtime DB via `read system://workspace`, then `read` the canonical URI you plan to change.\n\
         - You may `search` for duplicate or alias coverage, but search results must never change the canonical target URI.\n\
         - Use `create` only when that canonical URI is missing.\n\
         - If that canonical URI already exists and the new instruction refines the same topic, always `update` that same canonical node.\n\
         - Keep temporary task instructions and unverified guesses out of canonical long-term memory.\n\
         - In high-load turns, prioritize recall first and defer writes unless the durable fact is explicit.\n\
         - After writing, verify the result by reading back the canonical URI you changed."
    )
}

#[cfg(test)]
#[path = "zmemory_contract_tests.rs"]
mod tests;

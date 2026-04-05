use super::StablePreferenceMemory;
use super::stable_preference_contract_markdown;
use pretty_assertions::assert_eq;

#[test]
fn stable_preference_memory_uses_canonical_core_uris() {
    assert_eq!(
        vec![
            (
                StablePreferenceMemory::UserAddressPreference.uri(),
                StablePreferenceMemory::UserAddressPreference.topic(),
            ),
            (
                StablePreferenceMemory::AgentSelfReference.uri(),
                StablePreferenceMemory::AgentSelfReference.topic(),
            ),
            (
                StablePreferenceMemory::CollaborationAddressContract.uri(),
                StablePreferenceMemory::CollaborationAddressContract.topic(),
            ),
        ],
        vec![
            ("core://my_user", "the user's preferred form of address"),
            ("core://agent", "the assistant's preferred self-name"),
            (
                "core://agent/my_user",
                "the shared naming and addressing contract between user and assistant",
            ),
        ]
    );
}

#[test]
fn stable_preference_contract_markdown_describes_dedupe_and_update_rules() {
    let markdown = stable_preference_contract_markdown();

    assert!(markdown.contains("explicit, durable naming or addressing preferences"));
    assert!(markdown.contains("`core://my_user`"));
    assert!(markdown.contains("`core://agent`"));
    assert!(markdown.contains("`core://agent/my_user`"));
    assert!(markdown.contains("`read system://workspace`"));
    assert!(markdown.contains("`read` the canonical URI you plan to change"));
    assert!(markdown.contains("`search` for duplicate or alias coverage"));
    assert!(markdown.contains("stable identity layer"));
    assert!(markdown.contains("do not ask the user which path to use"));
    assert!(markdown.contains("must never change the canonical target URI"));
    assert!(markdown.contains("Use `create` only when that canonical URI is missing"));
    assert!(markdown.contains("always `update` that same canonical node"));
    assert!(markdown.contains("temporary task instructions"));
    assert!(markdown.contains("prioritize recall first"));
    assert!(markdown.contains("reading back the canonical URI"));
}

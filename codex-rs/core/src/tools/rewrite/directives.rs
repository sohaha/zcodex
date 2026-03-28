use codex_protocol::user_input::UserInput;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ToolRoutingDirectives {
    pub(crate) disable_auto_tldr_once: bool,
    pub(crate) force_raw_read: bool,
    pub(crate) force_raw_grep: bool,
    pub(crate) prefer_context_search: bool,
}

pub(crate) fn extract_tool_routing_directives(input: &[UserInput]) -> ToolRoutingDirectives {
    let normalized = input
        .iter()
        .filter_map(|item| match item {
            UserInput::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
        .replace('`', "");

    let mentions_tldr = normalized.contains("tldr");
    let explicit_raw_tldr = mentions_tldr
        && contains_any(
            &normalized,
            &[
                "不要 tldr",
                "不要tldr",
                "不用 tldr",
                "不用tldr",
                "别用 tldr",
                "别用tldr",
                "don't use tldr",
                "do not use tldr",
                "skip tldr",
            ],
        );

    ToolRoutingDirectives {
        disable_auto_tldr_once: explicit_raw_tldr,
        force_raw_read: contains_any(
            &normalized,
            &[
                "原文", "逐字", "verbatim", "literal", "raw read", "raw file",
            ],
        ),
        force_raw_grep: contains_any(
            &normalized,
            &[
                "ripgrep",
                "regex",
                "regexp",
                "正则",
                "精确 grep",
                "精确grep",
                "原始 grep",
                "原始grep",
            ],
        ),
        prefer_context_search: mentions_tldr
            && contains_any(
                &normalized,
                &[
                    "先用",
                    "优先",
                    "first",
                    "prefer",
                    "上下文",
                    "context",
                    "调用关系",
                    "调用链",
                    "call graph",
                    "impact",
                    "影响范围",
                ],
            ),
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::ToolRoutingDirectives;
    use super::extract_tool_routing_directives;
    use codex_protocol::user_input::UserInput;
    use pretty_assertions::assert_eq;

    #[test]
    fn extracts_tldr_first_context_directives_from_user_prompt() {
        let directives = extract_tool_routing_directives(&[UserInput::Text {
            text:
                "先用 tldr，不要先广泛读文件。分析 create_tldr_tool 的上下文、调用关系和影响范围。"
                    .to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            directives,
            ToolRoutingDirectives {
                disable_auto_tldr_once: false,
                force_raw_read: false,
                force_raw_grep: false,
                prefer_context_search: true,
            }
        );
    }

    #[test]
    fn extracts_explicit_raw_grep_directives() {
        let directives = extract_tool_routing_directives(&[UserInput::Text {
            text: "不要 tldr，按 regex 用 ripgrep 精确 grep。".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            directives,
            ToolRoutingDirectives {
                disable_auto_tldr_once: true,
                force_raw_read: false,
                force_raw_grep: true,
                prefer_context_search: false,
            }
        );
    }
}

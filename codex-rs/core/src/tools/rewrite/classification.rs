use codex_protocol::user_input::UserInput;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ProblemKind {
    #[default]
    Structural,
    Factual,
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolRoutingIntent {
    pub(crate) problem_kind: ProblemKind,
    pub(crate) disable_auto_tldr_once: bool,
    pub(crate) force_tldr: bool,
    pub(crate) force_raw_read: bool,
    pub(crate) force_raw_grep: bool,
    pub(crate) prefer_context_search: bool,
}

impl Default for ToolRoutingIntent {
    fn default() -> Self {
        Self {
            problem_kind: ProblemKind::Structural,
            disable_auto_tldr_once: false,
            force_tldr: false,
            force_raw_read: false,
            force_raw_grep: false,
            prefer_context_search: true,
        }
    }
}

pub(crate) fn classify_tool_routing_intent(input: &[UserInput]) -> ToolRoutingIntent {
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

    classify_normalized_text(&normalized)
}

fn classify_normalized_text(normalized: &str) -> ToolRoutingIntent {
    let mentions_tldr = normalized.contains("tldr");
    let disable_auto_tldr_once = mentions_tldr
        && contains_any(
            normalized,
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
    let force_tldr = mentions_tldr
        && !disable_auto_tldr_once
        && contains_any(
            normalized,
            &[
                "用 tldr",
                "用tldr",
                "先用 tldr",
                "先用tldr",
                "直接 tldr",
                "直接tldr",
                "tldr 看",
                "tldr分析",
                "tldr 分析",
                "use tldr",
                "analyze with tldr",
            ],
        );

    let force_raw_read = contains_any(
        normalized,
        &[
            "原文", "逐字", "verbatim", "literal", "raw read", "raw file",
        ],
    );
    let force_raw_grep = contains_any(
        normalized,
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
    );

    let structural = contains_any(
        normalized,
        &[
            "上下文",
            "调用",
            "调用关系",
            "被谁调用",
            "谁调用",
            "影响范围",
            "影响什么",
            "影响哪些",
            "定义在哪",
            "在哪里定义",
            "相似实现",
            "结构",
            "架构",
            "symbol",
            "context",
            "impact",
            "calls",
        ],
    );
    let factual = contains_any(
        normalized,
        &[
            "默认值",
            "default value",
            "默认配置",
            "feature gate",
            "cargo feature",
            "编译开关",
            "readme",
            "文档声明",
            "测试覆盖",
            "test coverage",
            "测试是否覆盖",
            "配置值",
        ],
    );
    let problem_kind = match (structural, factual) {
        (true, true) => ProblemKind::Mixed,
        (false, true) => ProblemKind::Factual,
        _ => ProblemKind::Structural,
    };

    ToolRoutingIntent {
        problem_kind,
        disable_auto_tldr_once,
        force_tldr,
        force_raw_read,
        force_raw_grep,
        prefer_context_search: !disable_auto_tldr_once
            && !matches!(problem_kind, ProblemKind::Factual),
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::ProblemKind;
    use super::ToolRoutingIntent;
    use super::classify_tool_routing_intent;
    use codex_protocol::user_input::UserInput;
    use pretty_assertions::assert_eq;

    #[test]
    fn directives_default_to_structural_context_search() {
        assert_eq!(
            ToolRoutingIntent::default(),
            ToolRoutingIntent {
                problem_kind: ProblemKind::Structural,
                disable_auto_tldr_once: false,
                force_tldr: false,
                force_raw_read: false,
                force_raw_grep: false,
                prefer_context_search: true,
            }
        );
    }

    #[test]
    fn classifies_plain_analysis_request_as_structural() {
        let directives = classify_tool_routing_intent(&[UserInput::Text {
            text: "分析 create_tldr_tool 的实现。".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            directives,
            ToolRoutingIntent {
                problem_kind: ProblemKind::Structural,
                disable_auto_tldr_once: false,
                force_tldr: false,
                force_raw_read: false,
                force_raw_grep: false,
                prefer_context_search: true,
            }
        );
    }

    #[test]
    fn classifies_default_value_checks_as_factual() {
        let directives = classify_tool_routing_intent(&[UserInput::Text {
            text: "确认 auto_tldr_routing 的默认值和 cargo feature。".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            directives,
            ToolRoutingIntent {
                problem_kind: ProblemKind::Factual,
                disable_auto_tldr_once: false,
                force_tldr: false,
                force_raw_read: false,
                force_raw_grep: false,
                prefer_context_search: false,
            }
        );
    }

    #[test]
    fn classifies_structure_plus_fact_checks_as_mixed() {
        let directives = classify_tool_routing_intent(&[UserInput::Text {
            text: "先看 create_tldr_tool 的调用关系，再确认默认值和测试覆盖。".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            directives,
            ToolRoutingIntent {
                problem_kind: ProblemKind::Mixed,
                disable_auto_tldr_once: false,
                force_tldr: false,
                force_raw_read: false,
                force_raw_grep: false,
                prefer_context_search: true,
            }
        );
    }

    #[test]
    fn extracts_tldr_first_context_directives_from_user_prompt() {
        let directives = classify_tool_routing_intent(&[UserInput::Text {
            text:
                "先用 tldr，不要先广泛读文件。分析 create_tldr_tool 的上下文、调用关系和影响范围。"
                    .to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            directives,
            ToolRoutingIntent {
                problem_kind: ProblemKind::Structural,
                disable_auto_tldr_once: false,
                force_tldr: true,
                force_raw_read: false,
                force_raw_grep: false,
                prefer_context_search: true,
            }
        );
    }

    #[test]
    fn extracts_explicit_raw_grep_directives() {
        let directives = classify_tool_routing_intent(&[UserInput::Text {
            text: "不要 tldr，按 regex 用 ripgrep 精确 grep。".to_string(),
            text_elements: Vec::new(),
        }]);

        assert_eq!(
            directives,
            ToolRoutingIntent {
                problem_kind: ProblemKind::Structural,
                disable_auto_tldr_once: true,
                force_tldr: false,
                force_raw_read: false,
                force_raw_grep: true,
                prefer_context_search: false,
            }
        );
    }
}

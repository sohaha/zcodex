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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutoTldrDirective {
    DisableOnce,
    ForceTldr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct ToolRoutingUpdate {
    problem_kind: Option<ProblemKind>,
    auto_tldr: Option<AutoTldrDirective>,
    force_raw_read: bool,
    force_raw_grep: bool,
}

pub(crate) fn classify_tool_routing_intent(input: &[UserInput]) -> ToolRoutingIntent {
    apply_tool_routing_updates(ToolRoutingIntent::default(), input)
}

pub(crate) fn apply_tool_routing_updates(
    mut current: ToolRoutingIntent,
    input: &[UserInput],
) -> ToolRoutingIntent {
    for normalized in normalized_text_inputs(input) {
        current = apply_routing_update(current, classify_normalized_text(&normalized));
    }
    current
}

fn apply_routing_update(
    mut current: ToolRoutingIntent,
    update: ToolRoutingUpdate,
) -> ToolRoutingIntent {
    if let Some(problem_kind) = update.problem_kind {
        current.problem_kind = problem_kind;
    }

    match update.auto_tldr {
        Some(AutoTldrDirective::DisableOnce) => {
            current.disable_auto_tldr_once = true;
            current.force_tldr = false;
        }
        Some(AutoTldrDirective::ForceTldr) => {
            current.disable_auto_tldr_once = false;
            current.force_tldr = true;
            current.force_raw_read = false;
            current.force_raw_grep = false;
        }
        None => {}
    }

    if update.force_raw_read {
        current.force_raw_read = true;
        current.force_raw_grep = false;
        current.force_tldr = false;
    }
    if update.force_raw_grep {
        current.force_raw_grep = true;
        current.force_raw_read = false;
        current.force_tldr = false;
    }

    current.prefer_context_search = !current.disable_auto_tldr_once
        && !current.force_raw_grep
        && !matches!(current.problem_kind, ProblemKind::Factual);
    current
}

fn normalized_text_inputs(input: &[UserInput]) -> Vec<String> {
    input
        .iter()
        .filter_map(|item| match item {
            UserInput::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .map(|text| text.to_lowercase().replace('`', ""))
        .collect()
}

fn classify_normalized_text(normalized: &str) -> ToolRoutingUpdate {
    let mentions_ztldr = normalized.contains("ztldr") || normalized.contains("tldr");
    let auto_tldr = if mentions_ztldr
        && contains_any(
            normalized,
            &[
                "不要 ztldr",
                "不要ztldr",
                "不要 tldr",
                "不要tldr",
                "不用 ztldr",
                "不用ztldr",
                "不用 tldr",
                "不用tldr",
                "别用 ztldr",
                "别用ztldr",
                "别用 tldr",
                "别用tldr",
                "don't use ztldr",
                "don't use tldr",
                "do not use ztldr",
                "do not use tldr",
                "skip ztldr",
                "skip tldr",
            ],
        ) {
        Some(AutoTldrDirective::DisableOnce)
    } else if mentions_ztldr
        && contains_any(
            normalized,
            &[
                "用 ztldr",
                "用ztldr",
                "用 tldr",
                "用tldr",
                "先用 ztldr",
                "先用ztldr",
                "先用 tldr",
                "先用tldr",
                "直接 ztldr",
                "直接ztldr",
                "直接 tldr",
                "直接tldr",
                "ztldr 看",
                "ztldr分析",
                "ztldr 分析",
                "tldr 看",
                "tldr分析",
                "tldr 分析",
                "use ztldr",
                "use tldr",
                "analyze with tldr",
            ],
        )
    {
        Some(AutoTldrDirective::ForceTldr)
    } else {
        None
    };

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
        (true, true) => Some(ProblemKind::Mixed),
        (false, true) => Some(ProblemKind::Factual),
        (true, false) => Some(ProblemKind::Structural),
        (false, false) => None,
    };

    ToolRoutingUpdate {
        problem_kind,
        auto_tldr,
        force_raw_read,
        force_raw_grep,
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::ProblemKind;
    use super::ToolRoutingIntent;
    use super::apply_tool_routing_updates;
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
            text: "不要 ztldr，按 regex 用 ripgrep 精确 grep。".to_string(),
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

    #[test]
    fn neutral_follow_up_preserves_existing_directives() {
        let directives = apply_tool_routing_updates(
            ToolRoutingIntent {
                problem_kind: ProblemKind::Structural,
                disable_auto_tldr_once: true,
                force_tldr: false,
                force_raw_read: false,
                force_raw_grep: true,
                prefer_context_search: false,
            },
            &[UserInput::Text {
                text: "继续看一下。".to_string(),
                text_elements: Vec::new(),
            }],
        );

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

    #[test]
    fn latest_follow_up_overrides_conflicting_prior_directives() {
        let directives = classify_tool_routing_intent(&[
            UserInput::Text {
                text: "不要 ztldr，按 regex 用 ripgrep。".to_string(),
                text_elements: Vec::new(),
            },
            UserInput::Text {
                text: "还是用 ztldr 看调用关系。".to_string(),
                text_elements: Vec::new(),
            },
        ]);

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
}

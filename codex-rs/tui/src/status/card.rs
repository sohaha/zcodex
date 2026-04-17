use crate::history_cell::CompositeHistoryCell;
use crate::history_cell::HistoryCell;
use crate::history_cell::PlainHistoryCell;
use crate::history_cell::with_border_with_inner_width;
use crate::legacy_core::config::Config;
use crate::version::CODEX_CLI_VERSION;
use chrono::DateTime;
use chrono::Local;
use codex_model_provider_info::WireApi;
use codex_protocol::ThreadId;
use codex_protocol::account::PlanType;
use codex_protocol::openai_models::ReasoningEffort;
use codex_protocol::protocol::AskForApproval;
use codex_protocol::protocol::NetworkAccess;
use codex_protocol::protocol::SandboxPolicy;
use codex_protocol::protocol::TokenUsage;
use codex_protocol::protocol::TokenUsageInfo;
use codex_utils_sandbox_summary::summarize_sandbox_policy;
use ratatui::prelude::*;
use ratatui::style::Stylize;
use std::collections::BTreeSet;
use std::path::PathBuf;
use url::Url;

use super::account::StatusAccountDisplay;
use super::format::FieldFormatter;
use super::format::line_display_width;
use super::format::push_label;
use super::format::truncate_line_to_width;
use super::helpers::compose_account_display;
use super::helpers::compose_model_display;
use super::helpers::format_directory_display;
use super::helpers::format_tokens_compact;
use super::rate_limits::RateLimitSnapshotDisplay;
use super::rate_limits::StatusRateLimitData;
use super::rate_limits::StatusRateLimitRow;
use super::rate_limits::StatusRateLimitValue;
use super::rate_limits::compose_rate_limit_data;
use super::rate_limits::compose_rate_limit_data_many;
use super::rate_limits::format_status_limit_summary;
use super::rate_limits::render_status_limit_progress_bar;
use crate::wrapping::RtOptions;
use crate::wrapping::adaptive_wrap_lines;
use std::sync::Arc;
use std::sync::RwLock;

#[derive(Debug, Clone)]
struct StatusContextWindowData {
    percent_remaining: i64,
    tokens_in_context: i64,
    window: i64,
}

#[derive(Debug, Clone)]
pub(crate) struct StatusTokenUsageData {
    total: i64,
    input: i64,
    output: i64,
    context_window: Option<StatusContextWindowData>,
}

#[derive(Debug)]
struct StatusRateLimitState {
    rate_limits: StatusRateLimitData,
    refreshing_rate_limits: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct StatusHistoryHandle {
    rate_limit_state: Arc<RwLock<StatusRateLimitState>>,
}

impl StatusHistoryHandle {
    pub(crate) fn finish_rate_limit_refresh(
        &self,
        rate_limits: &[RateLimitSnapshotDisplay],
        now: DateTime<Local>,
    ) {
        let rate_limits = if rate_limits.len() <= 1 {
            compose_rate_limit_data(rate_limits.first(), now)
        } else {
            compose_rate_limit_data_many(rate_limits, now)
        };
        #[expect(clippy::expect_used)]
        let mut state = self
            .rate_limit_state
            .write()
            .expect("status history rate-limit state poisoned");
        state.rate_limits = rate_limits;
        state.refreshing_rate_limits = false;
    }
}

#[derive(Debug)]
struct StatusHistoryCell {
    model_name: String,
    model_details: Vec<String>,
    directory: PathBuf,
    permissions: String,
    agents_summary: String,
    collaboration_mode: Option<String>,
    model_provider: Option<String>,
    account: Option<StatusAccountDisplay>,
    thread_name: Option<String>,
    session_id: Option<String>,
    forked_from: Option<String>,
    token_usage: StatusTokenUsageData,
    rate_limit_state: Arc<RwLock<StatusRateLimitState>>,
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn new_status_output(
    config: &Config,
    account_display: Option<&StatusAccountDisplay>,
    token_info: Option<&TokenUsageInfo>,
    total_usage: &TokenUsage,
    session_id: &Option<ThreadId>,
    thread_name: Option<String>,
    forked_from: Option<ThreadId>,
    rate_limits: Option<&RateLimitSnapshotDisplay>,
    _plan_type: Option<PlanType>,
    now: DateTime<Local>,
    model_name: &str,
    collaboration_mode: Option<&str>,
    reasoning_effort_override: Option<Option<ReasoningEffort>>,
) -> CompositeHistoryCell {
    let snapshots = rate_limits.map(std::slice::from_ref).unwrap_or_default();
    new_status_output_with_rate_limits(
        config,
        account_display,
        token_info,
        total_usage,
        session_id,
        thread_name,
        forked_from,
        snapshots,
        _plan_type,
        now,
        model_name,
        collaboration_mode,
        reasoning_effort_override,
        /*refreshing_rate_limits*/ false,
    )
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn new_status_output_with_rate_limits(
    config: &Config,
    account_display: Option<&StatusAccountDisplay>,
    token_info: Option<&TokenUsageInfo>,
    total_usage: &TokenUsage,
    session_id: &Option<ThreadId>,
    thread_name: Option<String>,
    forked_from: Option<ThreadId>,
    rate_limits: &[RateLimitSnapshotDisplay],
    _plan_type: Option<PlanType>,
    now: DateTime<Local>,
    model_name: &str,
    collaboration_mode: Option<&str>,
    reasoning_effort_override: Option<Option<ReasoningEffort>>,
    refreshing_rate_limits: bool,
) -> CompositeHistoryCell {
    new_status_output_with_rate_limits_handle(
        config,
        account_display,
        token_info,
        total_usage,
        session_id,
        thread_name,
        forked_from,
        rate_limits,
        _plan_type,
        now,
        model_name,
        collaboration_mode,
        reasoning_effort_override,
        "无".to_string(),
        refreshing_rate_limits,
    )
    .0
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn new_status_output_with_rate_limits_handle(
    config: &Config,
    account_display: Option<&StatusAccountDisplay>,
    token_info: Option<&TokenUsageInfo>,
    total_usage: &TokenUsage,
    session_id: &Option<ThreadId>,
    thread_name: Option<String>,
    forked_from: Option<ThreadId>,
    rate_limits: &[RateLimitSnapshotDisplay],
    _plan_type: Option<PlanType>,
    now: DateTime<Local>,
    model_name: &str,
    collaboration_mode: Option<&str>,
    reasoning_effort_override: Option<Option<ReasoningEffort>>,
    agents_summary: String,
    refreshing_rate_limits: bool,
) -> (CompositeHistoryCell, StatusHistoryHandle) {
    let command = PlainHistoryCell::new(vec!["/status".magenta().into()]);
    let (card, handle) = StatusHistoryCell::new(
        config,
        account_display,
        token_info,
        total_usage,
        session_id,
        thread_name,
        forked_from,
        rate_limits,
        _plan_type,
        now,
        model_name,
        collaboration_mode,
        reasoning_effort_override,
        agents_summary,
        refreshing_rate_limits,
    );

    (
        CompositeHistoryCell::new(vec![Box::new(command), Box::new(card)]),
        handle,
    )
}

impl StatusHistoryCell {
    #[allow(clippy::too_many_arguments)]
    fn new(
        config: &Config,
        account_display: Option<&StatusAccountDisplay>,
        token_info: Option<&TokenUsageInfo>,
        total_usage: &TokenUsage,
        session_id: &Option<ThreadId>,
        thread_name: Option<String>,
        forked_from: Option<ThreadId>,
        rate_limits: &[RateLimitSnapshotDisplay],
        _plan_type: Option<PlanType>,
        now: DateTime<Local>,
        model_name: &str,
        collaboration_mode: Option<&str>,
        reasoning_effort_override: Option<Option<ReasoningEffort>>,
        agents_summary: String,
        refreshing_rate_limits: bool,
    ) -> (Self, StatusHistoryHandle) {
        let mut config_entries = vec![
            ("workdir", config.cwd.display().to_string()),
            ("model", model_name.to_string()),
            ("provider", config.model_provider_id.clone()),
            (
                "approval",
                config.permissions.approval_policy.value().to_string(),
            ),
            (
                "sandbox",
                summarize_sandbox_policy(config.permissions.sandbox_policy.get()),
            ),
        ];
        if config.model_provider.wire_api == WireApi::Responses {
            let effort_value = reasoning_effort_override
                .unwrap_or(config.model_reasoning_effort)
                .map(|effort| effort.to_string())
                .unwrap_or_else(|| "none".to_string());
            config_entries.push(("reasoning effort", effort_value));
            config_entries.push((
                "reasoning summaries",
                config
                    .model_reasoning_summary
                    .map(|summary| summary.to_string())
                    .unwrap_or_else(|| "auto".to_string()),
            ));
        }
        let (model_name, model_details) = compose_model_display(model_name, &config_entries);
        let approval = config_entries
            .iter()
            .find(|(k, _)| *k == "approval")
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| "<unknown>".to_string());
        let sandbox = match config.permissions.sandbox_policy.get() {
            SandboxPolicy::DangerFullAccess => "danger-full-access".to_string(),
            SandboxPolicy::ReadOnly { .. } => "read-only".to_string(),
            SandboxPolicy::WorkspaceWrite {
                network_access: true,
                ..
            } => "workspace-write with network access".to_string(),
            SandboxPolicy::WorkspaceWrite { .. } => "workspace-write".to_string(),
            SandboxPolicy::ExternalSandbox { network_access } => {
                if matches!(network_access, NetworkAccess::Enabled) {
                    "external-sandbox (network access enabled)".to_string()
                } else {
                    "external-sandbox".to_string()
                }
            }
        };
        let permissions = if config.permissions.approval_policy.value() == AskForApproval::OnRequest
            && *config.permissions.sandbox_policy.get()
                == SandboxPolicy::new_workspace_write_policy()
        {
            "默认".to_string()
        } else if config.permissions.approval_policy.value() == AskForApproval::Never
            && *config.permissions.sandbox_policy.get() == SandboxPolicy::DangerFullAccess
        {
            "完全访问".to_string()
        } else {
            format!("自定义 ({sandbox}, {approval})")
        };
        let agents_summary = agents_summary;
        let model_provider = format_model_provider(config);
        let account = compose_account_display(account_display);
        let session_id = session_id.as_ref().map(std::string::ToString::to_string);
        let forked_from = forked_from.map(|id| id.to_string());
        let default_usage = TokenUsage::default();
        let (context_usage, context_window) = match token_info {
            Some(info) => (&info.last_token_usage, info.model_context_window),
            None => (&default_usage, config.model_context_window),
        };
        let context_window = context_window.map(|window| StatusContextWindowData {
            percent_remaining: context_usage.percent_of_context_window_remaining(window),
            tokens_in_context: context_usage.tokens_in_context_window(),
            window,
        });

        let token_usage = StatusTokenUsageData {
            total: total_usage.blended_total(),
            input: total_usage.non_cached_input(),
            output: total_usage.output_tokens,
            context_window,
        };
        let rate_limits = if rate_limits.len() <= 1 {
            compose_rate_limit_data(rate_limits.first(), now)
        } else {
            compose_rate_limit_data_many(rate_limits, now)
        };
        let rate_limit_state = Arc::new(RwLock::new(StatusRateLimitState {
            rate_limits,
            refreshing_rate_limits,
        }));

        (
            Self {
                model_name,
                model_details,
                directory: config.cwd.to_path_buf(),
                permissions,
                agents_summary,
                collaboration_mode: collaboration_mode.map(ToString::to_string),
                model_provider,
                account,
                thread_name,
                session_id,
                forked_from,
                token_usage,
                rate_limit_state: rate_limit_state.clone(),
            },
            StatusHistoryHandle { rate_limit_state },
        )
    }

    fn token_usage_spans(&self) -> Vec<Span<'static>> {
        let total_fmt = format_tokens_compact(self.token_usage.total);
        let input_fmt = format_tokens_compact(self.token_usage.input);
        let output_fmt = format_tokens_compact(self.token_usage.output);

        vec![
            Span::from(total_fmt),
            Span::from(" 总计 "),
            Span::from(" (").dim(),
            Span::from(input_fmt).dim(),
            Span::from(" 输入").dim(),
            Span::from(" + ").dim(),
            Span::from(output_fmt).dim(),
            Span::from(" 输出").dim(),
            Span::from(")").dim(),
        ]
    }

    fn context_window_spans(&self) -> Option<Vec<Span<'static>>> {
        let context = self.token_usage.context_window.as_ref()?;
        let percent = context.percent_remaining;
        let used_fmt = format_tokens_compact(context.tokens_in_context);
        let window_fmt = format_tokens_compact(context.window);

        Some(vec![
            Span::from(format!("剩余 {percent}%")),
            Span::from(" (").dim(),
            Span::from(used_fmt).dim(),
            Span::from(" 已用 / ").dim(),
            Span::from(window_fmt).dim(),
            Span::from(")").dim(),
        ])
    }

    fn rate_limit_lines(
        &self,
        state: &StatusRateLimitState,
        available_inner_width: usize,
        formatter: &FieldFormatter,
    ) -> Vec<Line<'static>> {
        match &state.rate_limits {
            StatusRateLimitData::Available(rows_data) => {
                if rows_data.is_empty() {
                    return vec![formatter.line(
                        "限额",
                        vec![if state.refreshing_rate_limits {
                            Span::from("正在刷新缓存的限额...").dim()
                        } else {
                            Span::from("暂无数据").dim()
                        }],
                    )];
                }

                let mut lines =
                    self.rate_limit_row_lines(rows_data, available_inner_width, formatter);
                if state.refreshing_rate_limits {
                    lines.push(
                        formatter.line("提示", vec![Span::from("正在后台刷新限额...").dim()]),
                    );
                }
                lines
            }
            StatusRateLimitData::Stale(rows_data) => {
                let mut lines =
                    self.rate_limit_row_lines(rows_data, available_inner_width, formatter);
                lines.push(formatter.line(
                    "警告",
                    vec![Span::from(if state.refreshing_rate_limits {
                        "限额可能已过期 - 正在后台刷新..."
                    } else {
                        "限额可能已过期 - 请开始新回合刷新。"
                    })
                    .dim()],
                ));
                lines
            }
            StatusRateLimitData::Unavailable => {
                vec![formatter.line(
                    "限额",
                    vec![Span::from(if state.refreshing_rate_limits {
                        "正在刷新限额..."
                    } else {
                        "暂无数据"
                    })
                    .dim()],
                )]
            }
            StatusRateLimitData::Missing => {
                vec![formatter.line(
                    "限额",
                    vec![Span::from(if state.refreshing_rate_limits {
                        "正在刷新限额..."
                    } else {
                        "暂无数据"
                    })
                    .dim()],
                )]
            }
        }
    }

    fn rate_limit_row_lines(
        &self,
        rows: &[StatusRateLimitRow],
        available_inner_width: usize,
        formatter: &FieldFormatter,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(rows.len().saturating_mul(2));

        for row in rows {
            match &row.value {
                StatusRateLimitValue::Window {
                    percent_used,
                    resets_at,
                } => {
                    let percent_remaining = (100.0 - percent_used).clamp(0.0, 100.0);
                    let value_spans = vec![
                        Span::from(render_status_limit_progress_bar(percent_remaining)),
                        Span::from(" "),
                        Span::from(format_status_limit_summary(percent_remaining)),
                    ];
                    let base_spans = formatter.full_spans(row.label.as_str(), value_spans);
                    let base_line = Line::from(base_spans.clone());

                    if let Some(resets_at) = resets_at.as_ref() {
                        let resets_span = Span::from(format!("(重置 {resets_at})")).dim();
                        let mut inline_spans = base_spans.clone();
                        inline_spans.push(Span::from(" ").dim());
                        inline_spans.push(resets_span.clone());

                        if line_display_width(&Line::from(inline_spans.clone()))
                            <= available_inner_width
                        {
                            lines.push(Line::from(inline_spans));
                        } else {
                            lines.push(base_line);
                            lines.push(formatter.continuation(vec![resets_span]));
                        }
                    } else {
                        lines.push(base_line);
                    }
                }
                StatusRateLimitValue::Text(text) => {
                    let label = row.label.clone();
                    let spans =
                        formatter.full_spans(label.as_str(), vec![Span::from(text.clone())]);
                    lines.push(Line::from(spans));
                }
            }
        }

        lines
    }

    fn collect_rate_limit_labels(
        &self,
        state: &StatusRateLimitState,
        seen: &mut BTreeSet<String>,
        labels: &mut Vec<String>,
    ) {
        match &state.rate_limits {
            StatusRateLimitData::Available(rows) => {
                if rows.is_empty() {
                    push_label(labels, seen, "限额");
                } else {
                    for row in rows {
                        push_label(labels, seen, row.label.as_str());
                    }
                }
            }
            StatusRateLimitData::Stale(rows) => {
                for row in rows {
                    push_label(labels, seen, row.label.as_str());
                }
                push_label(labels, seen, "警告");
            }
            StatusRateLimitData::Unavailable | StatusRateLimitData::Missing => {
                push_label(labels, seen, "限额")
            }
        }
    }
}

impl HistoryCell for StatusHistoryCell {
    fn display_lines(&self, width: u16) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = Vec::new();
        lines.push(Line::from(vec![
            Span::from(format!("{}>_ ", FieldFormatter::INDENT)).dim(),
            Span::from("OpenAI Codex").bold(),
            Span::from(" ").dim(),
            Span::from(format!("(v{CODEX_CLI_VERSION})")).dim(),
        ]));
        lines.push(Line::from(Vec::<Span<'static>>::new()));

        let available_inner_width = usize::from(width.saturating_sub(4));
        if available_inner_width == 0 {
            return Vec::new();
        }

        let account_value = self.account.as_ref().map(|account| match account {
            StatusAccountDisplay::ChatGpt { email, plan } => match (email, plan) {
                (Some(email), Some(plan)) => format!("{email} ({plan})"),
                (Some(email), None) => email.clone(),
                (None, Some(plan)) => plan.clone(),
                (None, None) => "ChatGPT".to_string(),
            },
            StatusAccountDisplay::ApiKey => {
                "已配置 API key（运行 codex login 以使用 ChatGPT）".to_string()
            }
        });

        let mut labels: Vec<String> = vec!["模型", "目录", "权限", "Agents.md"]
            .into_iter()
            .map(str::to_string)
            .collect();
        let mut seen: BTreeSet<String> = labels.iter().cloned().collect();
        let thread_name = self.thread_name.as_deref().filter(|name| !name.is_empty());
        #[expect(clippy::expect_used)]
        let rate_limit_state = self
            .rate_limit_state
            .read()
            .expect("status history rate-limit state poisoned");

        if self.model_provider.is_some() {
            push_label(&mut labels, &mut seen, "模型提供方");
        }
        if account_value.is_some() {
            push_label(&mut labels, &mut seen, "账号");
        }
        if thread_name.is_some() {
            push_label(&mut labels, &mut seen, "线程名称");
        }
        if self.session_id.is_some() {
            push_label(&mut labels, &mut seen, "会话");
        }
        if self.session_id.is_some() && self.forked_from.is_some() {
            push_label(&mut labels, &mut seen, "分叉自");
        }
        if self.collaboration_mode.is_some() {
            push_label(&mut labels, &mut seen, "协作模式");
        }
        push_label(&mut labels, &mut seen, "Token 用量");
        if self.token_usage.context_window.is_some() {
            push_label(&mut labels, &mut seen, "上下文窗口");
        }

        self.collect_rate_limit_labels(&rate_limit_state, &mut seen, &mut labels);

        let formatter = FieldFormatter::from_labels(labels.iter().map(String::as_str));
        let value_width = formatter.value_width(available_inner_width);

        let note_first_line = Line::from(vec![
            Span::from("访问 ").cyan(),
            "https://chatgpt.com/codex/settings/usage"
                .cyan()
                .underlined(),
            Span::from(" 获取最新").cyan(),
        ]);
        let note_second_line = Line::from(vec![Span::from("限额与额度信息").cyan()]);
        let note_lines = adaptive_wrap_lines(
            [note_first_line, note_second_line],
            RtOptions::new(available_inner_width),
        );
        lines.extend(note_lines);
        lines.push(Line::from(Vec::<Span<'static>>::new()));

        let mut model_spans = vec![Span::from(self.model_name.clone())];
        if !self.model_details.is_empty() {
            model_spans.push(Span::from(" (").dim());
            model_spans.push(Span::from(self.model_details.join(", ")).dim());
            model_spans.push(Span::from(")").dim());
        }

        let directory_value = format_directory_display(&self.directory, Some(value_width));

        lines.push(formatter.line("模型", model_spans));
        if let Some(model_provider) = self.model_provider.as_ref() {
            lines.push(formatter.line("模型提供方", vec![Span::from(model_provider.clone())]));
        }
        lines.push(formatter.line("目录", vec![Span::from(directory_value)]));
        lines.push(formatter.line("权限", vec![Span::from(self.permissions.clone())]));
        lines.push(formatter.line("Agents.md", vec![Span::from(self.agents_summary.clone())]));

        if let Some(account_value) = account_value {
            lines.push(formatter.line("账号", vec![Span::from(account_value)]));
        }

        if let Some(thread_name) = thread_name {
            lines.push(formatter.line("线程名称", vec![Span::from(thread_name.to_string())]));
        }
        if let Some(collab_mode) = self.collaboration_mode.as_ref() {
            lines.push(formatter.line("协作模式", vec![Span::from(collab_mode.clone())]));
        }
        if let Some(session) = self.session_id.as_ref() {
            lines.push(formatter.line("会话", vec![Span::from(session.clone())]));
        }
        if self.session_id.is_some()
            && let Some(forked_from) = self.forked_from.as_ref()
        {
            lines.push(formatter.line("分叉自", vec![Span::from(forked_from.clone())]));
        }

        lines.push(Line::from(Vec::<Span<'static>>::new()));
        // Hide token usage only for ChatGPT subscribers
        if !matches!(self.account, Some(StatusAccountDisplay::ChatGpt { .. })) {
            lines.push(formatter.line("Token 用量", self.token_usage_spans()));
        }

        if let Some(spans) = self.context_window_spans() {
            lines.push(formatter.line("上下文窗口", spans));
        }

        lines.extend(self.rate_limit_lines(&rate_limit_state, available_inner_width, &formatter));

        let content_width = lines.iter().map(line_display_width).max().unwrap_or(0);
        let inner_width = content_width.min(available_inner_width);
        let truncated_lines: Vec<Line<'static>> = lines
            .into_iter()
            .map(|line| truncate_line_to_width(line, inner_width))
            .collect();

        with_border_with_inner_width(truncated_lines, inner_width)
    }
}

fn format_model_provider(config: &Config) -> Option<String> {
    let provider = &config.model_provider;
    let name = provider.name.as_deref().unwrap_or("");
    let provider_name = if name.is_empty() {
        config.model_provider_id.as_str()
    } else {
        name
    };
    let base_url = provider.base_url.as_deref().and_then(sanitize_base_url);
    let is_default_openai = provider.is_openai() && base_url.is_none();
    if is_default_openai {
        return None;
    }

    Some(match base_url {
        Some(base_url) => format!("{provider_name} - {base_url}"),
        None => provider_name.to_string(),
    })
}

fn sanitize_base_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let Ok(mut url) = Url::parse(trimmed) else {
        return None;
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    Some(url.to_string().trim_end_matches('/').to_string()).filter(|value| !value.is_empty())
}

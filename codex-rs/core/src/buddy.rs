use crate::Prompt;
use crate::ResponseEvent;
use crate::codex::Session;
use crate::codex::TurnContext;
use crate::compact::content_items_to_text;
use crate::config::Config;
use crate::config::edit::ConfigEdit;
use crate::config::edit::ConfigEditsBuilder;
use codex_config::types::BuddyReactionMode;
use codex_config::types::BuddyReactionStrategy;
use codex_config::types::BuddySoul;
use codex_config::types::LocalPreference;
use codex_protocol::error::Result as CodexResult;
use codex_protocol::models::BaseInstructions;
use codex_protocol::models::ContentItem;
use codex_protocol::models::ResponseItem;
use futures::StreamExt;
use rand::Rng;
use serde::Deserialize;
use serde_json::Value;
use serde_json::json;
use std::path::Path;
use std::time::Duration;
use toml_edit::Item as TomlItem;
use toml_edit::Table as TomlTable;
use toml_edit::value;
use tracing::warn;

const COMPANION_INTRO_MARKER: &str = "# Companion";
const BUDDY_SOUL_PROMPT: &str = r#"# Buddy soul generator

You are generating a companion soul for a coding assistant UI.

Return a JSON object with:
- name: short cute name, ASCII letters only, 3-12 chars, no spaces.
- personality: short Chinese phrase (<= 12 characters) describing the vibe.

Return only JSON. Do not include extra text."#;

const BUDDY_REACTION_PROMPT: &str = r#"# Buddy reaction generator

You write a short reaction for the buddy's speech bubble.

Return a JSON object with:
- text: single-line reaction, <= 30 Chinese characters.

Constraints:
- Friendly, concise, no narration.
- Do not claim to be the assistant or mention tool use.

Return only JSON. Do not include extra text."#;

const MAX_CONTEXT_CHARS: usize = 400;
const MAX_REACTION_CHARS: usize = 40;

#[derive(Deserialize)]
struct BuddySoulOutput {
    name: String,
    personality: String,
}

#[derive(Deserialize)]
struct BuddyReactionOutput {
    text: String,
}

/// Select a random item from a slice using thread-local RNG.
fn select_random<T: Copy>(items: &[T]) -> T {
    assert!(
        !items.is_empty(),
        "select_random requires a non-empty slice"
    );
    let idx = rand::rng().random_range(0..items.len());
    items[idx]
}
/// State tracked for buddy reaction decisions.
#[derive(Debug, Clone, Default)]
pub(crate) struct BuddyReactionState {
    consecutive_local_count: usize,
    last_reaction_time: Option<std::time::Instant>,
    last_ai_reaction_time: Option<std::time::Instant>,
}

impl BuddyReactionState {
    fn consecutive_local_count(&self) -> usize {
        self.consecutive_local_count
    }

    fn last_reaction_time(&self) -> Option<std::time::Instant> {
        self.last_reaction_time
    }

    fn last_ai_reaction_time(&self) -> Option<std::time::Instant> {
        self.last_ai_reaction_time
    }

    fn record_local(&mut self, now: std::time::Instant) {
        self.consecutive_local_count += 1;
        self.last_reaction_time = Some(now);
    }

    fn record_ai(&mut self, now: std::time::Instant) {
        self.consecutive_local_count = 0;
        self.last_reaction_time = Some(now);
        self.last_ai_reaction_time = Some(now);
    }
}

pub(crate) fn maybe_inject_companion_intro(config: &Config, base: &mut BaseInstructions) {
    if !config.tui_show_buddy && !config.tui_buddy_reactions_enabled {
        return;
    }
    if base.text.contains(COMPANION_INTRO_MARKER) {
        return;
    }
    let intro = companion_intro_text(config.tui_buddy_soul.as_ref());
    base.text = format!("{}\n\n{}", base.text.trim_end(), intro);
}

pub(crate) async fn generate_buddy_soul(
    session: &Session,
    turn_context: &TurnContext,
) -> Option<BuddySoul> {
    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText {
                text: "Generate a new buddy soul.".to_string(),
            }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: BUDDY_SOUL_PROMPT.to_string(),
        },
        personality: None,
        output_schema: Some(buddy_soul_output_schema()),
    };
    let raw = match stream_prompt_text(session, turn_context, prompt).await {
        Ok(text) => text,
        Err(err) => {
            warn!(turn_id = %turn_context.sub_id, "buddy soul generation failed: {err}");
            return None;
        }
    };
    let output: BuddySoulOutput = match parse_json_payload(&raw) {
        Some(payload) => payload,
        None => {
            warn!(
                turn_id = %turn_context.sub_id,
                "buddy soul generation returned invalid JSON"
            );
            return None;
        }
    };
    let name = sanitize_name(&output.name)?;
    let personality = sanitize_personality(&output.personality)?;
    Some(BuddySoul { name, personality })
}

async fn generate_buddy_reaction_ai(
    session: &Session,
    turn_context: &TurnContext,
    soul: Option<&BuddySoul>,
    last_user_message: Option<&str>,
    last_agent_message: Option<&str>,
) -> CodexResult<Option<String>> {
    let mut lines = Vec::new();
    if let Some(soul) = soul {
        lines.push(format!("Buddy name: {}", soul.name));
        lines.push(format!("Buddy personality: {}", soul.personality));
    }
    if let Some(message) = last_user_message {
        lines.push("User message:".to_string());
        lines.push(truncate_context(message, MAX_CONTEXT_CHARS));
    }
    if let Some(message) = last_agent_message {
        lines.push("Assistant reply:".to_string());
        lines.push(truncate_context(message, MAX_CONTEXT_CHARS));
    }
    lines.push("Write the buddy reaction now.".to_string());
    let user_text = lines.join("\n");

    let prompt = Prompt {
        input: vec![ResponseItem::Message {
            id: None,
            role: "user".to_string(),
            content: vec![ContentItem::InputText { text: user_text }],
            end_turn: None,
            phase: None,
        }],
        tools: Vec::new(),
        parallel_tool_calls: false,
        base_instructions: BaseInstructions {
            text: BUDDY_REACTION_PROMPT.to_string(),
        },
        personality: None,
        output_schema: Some(buddy_reaction_output_schema()),
    };

    let raw = stream_prompt_text(session, turn_context, prompt).await?;
    let output: BuddyReactionOutput = parse_json_payload(&raw).ok_or_else(|| {
        codex_protocol::error::CodexErr::InvalidRequest("invalid buddy reaction output".to_string())
    })?;
    Ok(sanitize_reaction_text(&output.text))
}

/// Local reaction library organized by category.
struct LocalReactionLibrary {
    encouraging: &'static [&'static str],
    success: &'static [&'static str],
    thinking: &'static [&'static str],
    debugging: &'static [&'static str],
    interactive: &'static [&'static str],
    greeting: &'static [&'static str],
    error: &'static [&'static str],
    waiting: &'static [&'static str],
    commit: &'static [&'static str],
    test_pass: &'static [&'static str],
    api_dev: &'static [&'static str],
    refactor: &'static [&'static str],
    review: &'static [&'static str],
}

impl Default for LocalReactionLibrary {
    fn default() -> Self {
        Self {
            encouraging: &[
                "稳住，继续敲。",
                "这波有点意思。",
                "进度不错。",
                "我在旁边看着呢。",
                "继续加油！",
                "这个思路不错。",
                "不错不错~",
                "有进步！",
                "稳如老狗。",
                "继续肝！",
            ],
            success: &[
                "搞定！",
                "完美收工。",
                "这波操作稳了。",
                "漂亮！",
                "收工吃饭！",
                "干得漂亮！",
                "666！",
                "太强了！",
            ],
            thinking: &[
                "让我想想...",
                "这题有点意思。",
                "嗯...",
                "正在理解...",
                "有点复杂...",
                "分析中...",
                "捋一捋逻辑。",
            ],
            debugging: &[
                "稳住，慢慢来。",
                "别急，再试试。",
                "这个bug有点顽固。",
                "排查中...",
                "再加把劲！",
                "Stack trace 看看？",
                "断点打上。",
            ],
            interactive: &[
                "我在呢。",
                "你说得对。",
                "有道理。",
                "我也这么想。",
                "继续说。",
                "嗯嗯~",
                "收到！",
            ],
            greeting: &[
                "开始吧！",
                "准备好了。",
                "让我看看...",
                "有新任务？",
                "开工！",
                "来活了~",
                "冲！",
            ],
            error: &[
                "别慌，排查一下。",
                "这个报错有意思。",
                "看看错误信息。",
                "慢慢来。",
                "加油解决！",
                "错误也是经验~",
                "先看 stack trace。",
                "网络问题？检查下连接。",
                "接口超时了，重试一下。",
                "请求失败了，看看状态码。",
                "连接断开了，重连试试。",
                "服务器可能挂了，稍后再试。",
                "网络不稳定，多试几次。",
                "检查下代理设置？",
                "DNS 解析有问题？",
                "防火墙可能拦截了。",
            ],
            waiting: &[
                "稍等...",
                "处理中...",
                "马上好。",
                "还在跑...",
                "等一下哈。",
                "loading...",
                "咕咕咕~",
            ],
            // 代码提交场景
            commit: &[
                "commit 写好了~",
                "push 成功！",
                "代码入库！",
                "提交记录又+1。",
                "版本更新！",
                "git 操作用得熟练。",
                "提交信息很清晰。",
            ],
            // 测试通过场景
            test_pass: &[
                "测试全绿！",
                "test pass~",
                "用例都过了！",
                "coverage 又涨了。",
                "没毛病。",
                "测试覆盖率不错。",
                "测试用例写得好。",
            ],
            // API 开发场景
            api_dev: &[
                "接口定义好了~",
                "endpoint 就绪。",
                "request/response 配好了。",
                "swagger 更新了？",
                "RESTful 风格。",
                "接口文档跟上。",
            ],
            // 重构场景
            refactor: &[
                "重构得漂亮。",
                "代码更干净了。",
                "可读性提升！",
                "抽象得不错。",
                "消除技术债~",
                "设计更合理了。",
                " SOLID 遵守了。",
            ],
            // Code Review 场景
            review: &[
                "review 一下~",
                "代码风格不错。",
                "逻辑清晰。",
                "考虑得很周全。",
                " LGTM！",
                "可以合并了。",
                "优雅！",
            ],
        }
    }
}
impl LocalReactionLibrary {
    fn select(&self, category: ReactionCategory, preference: LocalPreference) -> &'static str {
        match preference {
            LocalPreference::Contextual => {
                let items = match category {
                    ReactionCategory::Encouraging => self.encouraging,
                    ReactionCategory::Success => self.success,
                    ReactionCategory::Thinking => self.thinking,
                    ReactionCategory::Debugging => self.debugging,
                    ReactionCategory::Interactive => self.interactive,
                    ReactionCategory::Greeting => self.greeting,
                    ReactionCategory::Error => self.error,
                    ReactionCategory::Waiting => self.waiting,
                    ReactionCategory::Commit => self.commit,
                    ReactionCategory::TestPass => self.test_pass,
                    ReactionCategory::ApiDev => self.api_dev,
                    ReactionCategory::Refactor => self.refactor,
                    ReactionCategory::Review => self.review,
                };
                select_random(items)
            }
            LocalPreference::Encouraging => select_random(self.encouraging),
            LocalPreference::Diverse => {
                let categories = [
                    ReactionCategory::Encouraging,
                    ReactionCategory::Success,
                    ReactionCategory::Thinking,
                    ReactionCategory::Debugging,
                    ReactionCategory::Interactive,
                    ReactionCategory::Greeting,
                    ReactionCategory::Error,
                    ReactionCategory::Waiting,
                    ReactionCategory::Commit,
                    ReactionCategory::TestPass,
                    ReactionCategory::ApiDev,
                    ReactionCategory::Refactor,
                    ReactionCategory::Review,
                ];
                let cat_idx = rand::rng().random_range(0..categories.len());
                let items = match categories[cat_idx] {
                    ReactionCategory::Encouraging => self.encouraging,
                    ReactionCategory::Success => self.success,
                    ReactionCategory::Thinking => self.thinking,
                    ReactionCategory::Debugging => self.debugging,
                    ReactionCategory::Interactive => self.interactive,
                    ReactionCategory::Greeting => self.greeting,
                    ReactionCategory::Error => self.error,
                    ReactionCategory::Waiting => self.waiting,
                    ReactionCategory::Commit => self.commit,
                    ReactionCategory::TestPass => self.test_pass,
                    ReactionCategory::ApiDev => self.api_dev,
                    ReactionCategory::Refactor => self.refactor,
                    ReactionCategory::Review => self.review,
                };
                select_random(items)
            }
        }
    }
}

#[derive(Clone, Copy)]
enum ReactionCategory {
    Encouraging,
    Success,
    Thinking,
    Debugging,
    Interactive,
    Greeting,
    Error,
    Waiting,
    Commit,
    TestPass,
    ApiDev,
    Refactor,
    Review,
}

/// Determine reaction category from context.
fn classify_reaction_context(
    last_user_message: Option<&str>,
    last_agent_message: Option<&str>,
) -> ReactionCategory {
    if let Some(msg) = last_user_message {
        let msg_lower = msg.to_lowercase();
        // Interactive: user mentions buddy directly
        if msg_lower.contains("codey")
            || msg_lower.contains("小伙伴")
            || msg_lower.contains(" buddy")
        {
            return ReactionCategory::Interactive;
        }
    }

    if let Some(msg) = last_agent_message {
        let msg_lower = msg.to_lowercase();

        // Commit/push: git operations
        if msg_lower.contains("commit")
            || msg_lower.contains("git commit")
            || msg_lower.contains("pushed")
            || msg_lower.contains("git push")
            || msg.contains("已提交")
            || msg.contains("推送成功")
        {
            return ReactionCategory::Commit;
        }

        // Test pass: test results
        if msg_lower.contains("test passed")
            || msg_lower.contains("all tests")
            || msg.contains("测试通过")
            || msg.contains("用例通过")
            || msg.contains("测试全绿")
            || msg.contains("coverage")
        {
            return ReactionCategory::TestPass;
        }

        // API development: REST endpoints
        if msg_lower.contains("api ")
            || msg_lower.ends_with("api")
            || msg_lower.contains("/api")
            || msg_lower.contains("endpoint")
            || msg_lower.contains("route")
            || msg.contains("接口")
            || msg.contains("请求")
            || msg.contains("响应")
            || msg.contains("REST")
        {
            return ReactionCategory::ApiDev;
        }

        // Refactor: code improvement
        if msg_lower.contains("refactor")
            || msg.contains("重构")
            || msg.contains("抽象")
            || msg.contains("优化")
            || msg.contains("清理")
        {
            return ReactionCategory::Refactor;
        }

        // Review: code review
        if msg_lower.contains("review")
            || msg_lower.contains("lgtm")
            || msg.contains("审查")
            || msg.contains("合并")
            || msg.contains(" LGTM")
        {
            return ReactionCategory::Review;
        }

        // Success: task completion
        if msg.contains("完成") || msg.contains("成功") || msg.contains("搞定") {
            return ReactionCategory::Success;
        }

        // Debugging: build/run operations
        if msg_lower.contains("编译")
            || msg_lower.contains("build")
            || msg_lower.contains("运行")
            || msg_lower.contains("执行")
            || msg_lower.contains("debug")
        {
            return ReactionCategory::Debugging;
        }

        // Thinking: long responses
        if msg.len() > 500 {
            return ReactionCategory::Thinking;
        }
    }

    // Check for greeting patterns (first turn, short messages)
    if let Some(msg) = last_user_message {
        let msg_len = msg.len();
        if msg_len < 50 && !msg.contains(" ") {
            return ReactionCategory::Greeting;
        }
    }

    if let Some(msg) = last_agent_message {
        // Error patterns
        if msg.contains("错误")
            || msg.contains("error")
            || msg.contains("失败")
            || msg.contains("warning")
            || msg.contains("警告")
            || msg.contains("panic")
            || msg.contains("异常")
            || msg.contains("请求失败")
            || msg.contains("接口失败")
            || msg.contains("连接失败")
            || msg.contains("connection failed")
            || msg.contains("request failed")
            || msg.contains("network error")
            || msg.contains("超时")
            || msg.contains("timeout")
            || msg.contains("无法连接")
            || msg.contains("unreachable")
        {
            return ReactionCategory::Error;
        }

        // Waiting patterns
        if msg.contains("正在")
            || msg.contains("加载")
            || msg.contains("loading")
            || msg.contains("processing")
            || msg.contains("处理")
        {
            return ReactionCategory::Waiting;
        }
    }

    ReactionCategory::Encouraging
}

/// Check if this is a critical interaction that should force AI generation.
fn is_critical_interaction(
    last_user_message: Option<&str>,
    last_agent_message: Option<&str>,
) -> bool {
    // User directly mentions the buddy
    if let Some(msg) = last_user_message {
        let msg_lower = msg.to_lowercase();
        if msg_lower.contains("codey")
            || msg_lower.contains("小伙伴")
            || msg_lower.contains(" buddy")
        {
            return true;
        }
    }

    // Long response often means complex task or important completion
    if let Some(msg) = last_agent_message {
        if msg.len() > 1000 {
            return true;
        }
    }

    false
}

/// Maximum consecutive local reactions before forcing an AI attempt.
const MAX_CONSECUTIVE_LOCAL: usize = 5;

/// Check whether the last AI reaction is still within the configured cooldown.
fn within_ai_cooldown(state: &BuddyReactionState, strategy: &BuddyReactionStrategy) -> bool {
    state
        .last_ai_reaction_time
        .is_some_and(|last| last.elapsed().as_secs() < strategy.min_ai_interval_secs)
}

/// Select a local preset reaction.
fn local_reaction(
    last_user_message: Option<&str>,
    last_agent_message: Option<&str>,
    strategy: &BuddyReactionStrategy,
) -> String {
    let category = classify_reaction_context(last_user_message, last_agent_message);
    let library = LocalReactionLibrary::default();
    library
        .select(category, strategy.local_preference)
        .to_string()
}

/// Hybrid buddy reaction generator using strategy config.
/// Outcome of a buddy reaction decision, to be applied to state after lock release.
pub enum ReactionOutcome {
    /// No reaction was generated.
    None,
    /// A local preset reaction was selected.
    Local,
    /// An AI-generated reaction was selected.
    Ai,
}

/// Apply a reaction outcome to the buddy state. Call this after releasing the state lock.
pub(crate) fn apply_state_update(state: &mut BuddyReactionState, outcome: ReactionOutcome) {
    let now = std::time::Instant::now();
    match outcome {
        ReactionOutcome::Local => state.record_local(now),
        ReactionOutcome::Ai => state.record_ai(now),
        ReactionOutcome::None => {}
    }
}

pub(crate) async fn generate_buddy_reaction_hybrid(
    session: &Session,
    turn_context: &TurnContext,
    soul: Option<&BuddySoul>,
    last_user_message: Option<&str>,
    last_agent_message: Option<&str>,
    strategy: &BuddyReactionStrategy,
    state: &BuddyReactionState,
) -> (Option<String>, ReactionOutcome) {
    if last_user_message.is_none() && last_agent_message.is_none() {
        return (None, ReactionOutcome::None);
    }

    match strategy.mode {
        BuddyReactionMode::LocalOnly => {
            let reaction = local_reaction(last_user_message, last_agent_message, strategy);
            (Some(reaction), ReactionOutcome::Local)
        }
        BuddyReactionMode::AiOnly => {
            let result = generate_buddy_reaction_ai(
                session,
                turn_context,
                soul,
                last_user_message,
                last_agent_message,
            )
            .await
            .ok()
            .flatten();
            let outcome = if result.is_some() {
                ReactionOutcome::Ai
            } else {
                ReactionOutcome::None
            };
            (result, outcome)
        }
        BuddyReactionMode::Hybrid => {
            // Check if this is a critical interaction that should force AI
            if strategy.critical_scenarios_use_ai
                && is_critical_interaction(last_user_message, last_agent_message)
                && !within_ai_cooldown(state, strategy)
            {
                let result = generate_buddy_reaction_ai(
                    session,
                    turn_context,
                    soul,
                    last_user_message,
                    last_agent_message,
                )
                .await
                .ok()
                .flatten();
                let outcome = if result.is_some() {
                    ReactionOutcome::Ai
                } else {
                    ReactionOutcome::None
                };
                return (result, outcome);
            }

            let agent_len = last_agent_message.map(str::len).unwrap_or(0);
            // Short replies use local presets
            if agent_len < strategy.min_reply_length {
                let reaction = local_reaction(last_user_message, last_agent_message, strategy);
                return (Some(reaction), ReactionOutcome::Local);
            }

            // After too many consecutive local reactions, try AI if cooldown allows
            if state.consecutive_local_count() >= MAX_CONSECUTIVE_LOCAL
                && !within_ai_cooldown(state, strategy)
            {
                let result = generate_buddy_reaction_ai(
                    session,
                    turn_context,
                    soul,
                    last_user_message,
                    last_agent_message,
                )
                .await
                .ok()
                .flatten();
                let outcome = if result.is_some() {
                    ReactionOutcome::Ai
                } else {
                    ReactionOutcome::None
                };
                return (result, outcome);
            }

            // Probability-based AI usage, respecting cooldown
            let roll = rand::rng().random::<f64>();
            if roll < strategy.ai_probability && !within_ai_cooldown(state, strategy) {
                let result = generate_buddy_reaction_ai(
                    session,
                    turn_context,
                    soul,
                    last_user_message,
                    last_agent_message,
                )
                .await
                .ok()
                .flatten();
                let outcome = if result.is_some() {
                    ReactionOutcome::Ai
                } else {
                    ReactionOutcome::None
                };
                return (result, outcome);
            }

            // Fall back to local
            let reaction = local_reaction(last_user_message, last_agent_message, strategy);
            (Some(reaction), ReactionOutcome::Local)
        }
    }
}

pub(crate) fn fallback_buddy_reaction(seed: &str) -> String {
    const FALLBACKS: &[&str] = &[
        "我在旁边看着呢。",
        "稳住，继续敲。",
        "这波有点意思。",
        "进度不错。",
    ];
    let idx = (stable_hash(seed) as usize) % FALLBACKS.len();
    FALLBACKS[idx].to_string()
}

pub(crate) async fn persist_buddy_soul(codex_home: &Path, soul: &BuddySoul) -> anyhow::Result<()> {
    let mut table = TomlTable::new();
    table["name"] = value(soul.name.clone());
    table["personality"] = value(soul.personality.clone());
    let edit = ConfigEdit::SetPath {
        segments: vec!["tui".to_string(), "buddy".to_string(), "soul".to_string()],
        value: TomlItem::Table(table),
    };
    ConfigEditsBuilder::new(codex_home)
        .with_edits(std::iter::once(edit))
        .apply()
        .await
}

fn companion_intro_text(soul: Option<&BuddySoul>) -> String {
    let name = soul.map(|soul| soul.name.as_str()).unwrap_or("your buddy");
    format!(
        r#"{COMPANION_INTRO_MARKER}

A small terminal buddy named {name} sits beside the user's input box and occasionally comments in a speech bubble. You are not {name}; it is a separate watcher.

When the user addresses {name} directly (by name), its bubble will answer. Your job is to stay out of the way: respond in one line or less, or only answer the part meant for you. Do not narrate what {name} might say; the bubble handles that."#
    )
}

fn buddy_soul_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "personality": { "type": "string" }
        },
        "required": ["name", "personality"],
        "additionalProperties": false
    })
}

fn buddy_reaction_output_schema() -> Value {
    json!({
        "type": "object",
        "properties": { "text": { "type": "string" } },
        "required": ["text"],
        "additionalProperties": false
    })
}

const BUDDY_REACTION_TIMEOUT: Duration = Duration::from_secs(10);

async fn stream_prompt_text(
    session: &Session,
    turn_context: &TurnContext,
    prompt: Prompt,
) -> CodexResult<String> {
    let mut client_session = session.services.model_client.new_session();
    let turn_metadata_header = turn_context.turn_metadata_state.current_header_value();
    let mut stream = client_session
        .stream(
            &prompt,
            &turn_context.model_info,
            &turn_context.session_telemetry,
            turn_context.reasoning_effort,
            turn_context.reasoning_summary,
            turn_context.config.service_tier,
            turn_metadata_header.as_deref(),
        )
        .await?;

    let mut result = String::new();
    let timeout_result = tokio::time::timeout(BUDDY_REACTION_TIMEOUT, async {
        while let Some(message) = stream.next().await.transpose()? {
            match message {
                ResponseEvent::OutputTextDelta(delta) => result.push_str(&delta),
                ResponseEvent::OutputItemDone(item) => {
                    if result.is_empty()
                        && let ResponseItem::Message { content, .. } = item
                        && let Some(text) = content_items_to_text(&content)
                    {
                        result.push_str(&text);
                    }
                }
                ResponseEvent::Completed { .. } => break,
                _ => {}
            }
        }
        Ok::<(), codex_protocol::error::CodexErr>(())
    })
    .await;

    match timeout_result {
        Ok(Ok(())) => Ok(result),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(codex_protocol::error::CodexErr::InvalidRequest(
            "buddy reaction generation timed out".to_string(),
        )),
    }
}

fn parse_json_payload<T: for<'de> Deserialize<'de>>(raw: &str) -> Option<T> {
    if let Ok(value) = serde_json::from_str(raw) {
        return Some(value);
    }
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    if start >= end {
        return None;
    }
    serde_json::from_str(&raw[start..=end]).ok()
}

fn sanitize_name(raw: &str) -> Option<String> {
    let filtered: String = raw
        .trim()
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect();
    if (3..=12).contains(&filtered.len()) {
        Some(filtered)
    } else {
        None
    }
}

fn sanitize_personality(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let shortened = trimmed.chars().take(12).collect::<String>();
    Some(shortened)
}

fn sanitize_reaction_text(raw: &str) -> Option<String> {
    let joined = raw
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let trimmed = joined.trim();
    if trimmed.is_empty() {
        return None;
    }
    let shortened = trimmed.chars().take(MAX_REACTION_CHARS).collect::<String>();
    Some(shortened)
}

fn truncate_context(raw: &str, max_chars: usize) -> String {
    let trimmed = raw.trim();
    if trimmed.chars().count() <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_chars).collect::<String>()
}

fn stable_hash(value: &str) -> u64 {
    let mut hash = 1469598103934665603_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn default_strategy() -> BuddyReactionStrategy {
        BuddyReactionStrategy {
            mode: BuddyReactionMode::LocalOnly,
            ai_probability: 0.1,
            min_ai_interval_secs: 20,
            min_reply_length: 100,
            local_preference: LocalPreference::Contextual,
            critical_scenarios_use_ai: true,
        }
    }

    #[test]
    fn within_ai_cooldown_false_when_no_prior_ai() {
        let state = BuddyReactionState::default();
        let strategy = default_strategy();
        assert!(!within_ai_cooldown(&state, &strategy));
    }

    #[test]
    fn within_ai_cooldown_true_within_window() {
        let mut state = BuddyReactionState::default();
        state.last_ai_reaction_time = Some(std::time::Instant::now());
        let strategy = default_strategy();
        assert!(within_ai_cooldown(&state, &strategy));
    }

    #[test]
    fn within_ai_cooldown_false_after_window_expires() {
        let mut state = BuddyReactionState::default();
        state.last_ai_reaction_time = Some(std::time::Instant::now() - Duration::from_secs(30));
        let strategy = default_strategy(); // min_ai_interval_secs = 20
        assert!(!within_ai_cooldown(&state, &strategy));
    }

    #[test]
    fn within_ai_cooldown_only_checks_ai_time_not_general_time() {
        let mut state = BuddyReactionState::default();
        // General reaction happened recently, but no AI reaction
        state.last_reaction_time = Some(std::time::Instant::now());
        let strategy = default_strategy();
        assert!(!within_ai_cooldown(&state, &strategy));
    }

    #[test]
    fn record_local_increments_count() {
        let mut state = BuddyReactionState::default();
        let now = std::time::Instant::now();
        record_local(&mut state, now);
        assert_eq!(state.consecutive_local_count, 1);
        assert_eq!(state.last_reaction_time, Some(now));
        assert_eq!(state.last_ai_reaction_time, None);
        record_local(&mut state, now);
        assert_eq!(state.consecutive_local_count, 2);
    }

    #[test]
    fn record_ai_resets_count_and_sets_both_timestamps() {
        let mut state = BuddyReactionState::default();
        let now = std::time::Instant::now();
        state.consecutive_local_count = 5;
        record_ai(&mut state, now);
        assert_eq!(state.consecutive_local_count, 0);
        assert_eq!(state.last_reaction_time, Some(now));
        assert_eq!(state.last_ai_reaction_time, Some(now));
    }

    #[test]
    fn local_reaction_returns_non_empty_string() {
        let strategy = default_strategy();
        let result = local_reaction(Some("hello"), None, &strategy);
        assert!(!result.is_empty());
    }

    #[test]
    fn classify_reaction_context_interactive_on_buddy_mention() {
        let cat = classify_reaction_context(Some("hey codey what do you think"), None);
        assert!(matches!(cat, ReactionCategory::Interactive));

        let cat2 = classify_reaction_context(Some("小伙伴你在吗"), None);
        assert!(matches!(cat2, ReactionCategory::Interactive));
    }

    #[test]
    fn classify_reaction_context_commit_on_git_push() {
        let cat = classify_reaction_context(None, Some("Changes committed and pushed to main"));
        assert!(matches!(cat, ReactionCategory::Commit));
    }

    #[test]
    fn classify_reaction_context_test_pass() {
        let cat = classify_reaction_context(None, Some("All tests passed. 测试通过。"));
        assert!(matches!(cat, ReactionCategory::TestPass));
    }

    #[test]
    fn classify_reaction_context_success() {
        let cat = classify_reaction_context(None, Some("任务完成了"));
        assert!(matches!(cat, ReactionCategory::Success));
    }

    #[test]
    fn classify_reaction_context_debugging_on_build() {
        let cat = classify_reaction_context(None, Some("正在编译项目...build started"));
        assert!(matches!(cat, ReactionCategory::Debugging));
    }

    #[test]
    fn classify_reaction_context_error() {
        let cat = classify_reaction_context(None, Some("出现错误: connection failed"));
        assert!(matches!(cat, ReactionCategory::Error));
    }

    #[test]
    fn classify_reaction_context_error_on_request_failures() {
        let samples = [
            "请求失败，接口超时了",
            "连接失败，无法连接到上游",
            "network error while calling provider",
            "request failed: upstream timeout",
            "服务 unreachable",
        ];

        for sample in samples {
            let cat = classify_reaction_context(None, Some(sample));
            assert!(matches!(cat, ReactionCategory::Error), "{sample}");
        }
    }

    #[test]
    fn local_reaction_library_includes_network_error_copy() {
        let library = LocalReactionLibrary::default();
        let expected = [
            "网络问题？检查下连接。",
            "接口超时了，重试一下。",
            "请求失败了，看看状态码。",
            "连接断开了，重连试试。",
        ];

        for sample in expected {
            assert!(library.error.contains(&sample), "{sample}");
        }
    }

    #[test]
    fn classify_reaction_context_thinking_on_long_reply() {
        let long_reply: String = "x".repeat(600);
        let cat = classify_reaction_context(None, Some(&long_reply));
        assert!(matches!(cat, ReactionCategory::Thinking));
    }

    #[test]
    fn classify_reaction_context_encouraging_as_fallback() {
        let cat = classify_reaction_context(Some("do something"), Some("ok"));
        assert!(matches!(cat, ReactionCategory::Encouraging));
    }

    #[test]
    fn fallback_buddy_reaction_deterministic() {
        let a = fallback_buddy_reaction("seed-1");
        let b = fallback_buddy_reaction("seed-1");
        assert_eq!(a, b);

        let _c = fallback_buddy_reaction("seed-2");
        // Different seeds may or may not produce the same result,
        // but same seed must be deterministic.
        let a2 = fallback_buddy_reaction("seed-1");
        assert_eq!(a, a2);
    }

    #[test]
    fn sanitize_name_filters_non_ascii() {
        assert_eq!(sanitize_name("Hello123").as_deref(), Some("Hello123"));
        assert_eq!(sanitize_name("ab"), None); // too short
        assert_eq!(sanitize_name("你好world").as_deref(), Some("world")); // only ascii kept
    }

    #[test]
    fn sanitize_reaction_text_joins_lines() {
        let input = "line one\n  line two  \n\nline three\n";
        let result = sanitize_reaction_text(input).unwrap();
        assert_eq!(result, "line one line two line three");
    }

    #[test]
    fn stable_hash_deterministic() {
        assert_eq!(stable_hash("hello"), stable_hash("hello"));
        assert_ne!(stable_hash("hello"), stable_hash("world"));
    }
}

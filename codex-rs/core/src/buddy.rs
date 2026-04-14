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

/// Time utility for consistent time access.
fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

/// Select a random item from a slice using current time.
fn select_random<T: Copy>(items: &[T]) -> T {
    let idx = (current_time_ms() as usize) % items.len();
    items[idx]
}

/// State tracked for buddy reaction decisions.
#[derive(Debug, Clone, Default)]
pub(crate) struct BuddyReactionState {
    pub(crate) consecutive_local_count: usize,
    pub(crate) last_reaction_time: Option<std::time::Instant>,
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
                    ReactionCategory::Commit,
                    ReactionCategory::TestPass,
                    ReactionCategory::ApiDev,
                    ReactionCategory::Refactor,
                    ReactionCategory::Review,
                ];
                let cat_idx = (current_time_ms() as usize) % categories.len();
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
        if msg_lower.contains("api")
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

/// Hybrid buddy reaction generator using strategy config.
pub(crate) async fn generate_buddy_reaction_hybrid(
    session: &Session,
    turn_context: &TurnContext,
    soul: Option<&BuddySoul>,
    last_user_message: Option<&str>,
    last_agent_message: Option<&str>,
    strategy: &BuddyReactionStrategy,
) -> Option<String> {
    if last_user_message.is_none() && last_agent_message.is_none() {
        return None;
    }

    match strategy.mode {
        BuddyReactionMode::LocalOnly => {
            let category = classify_reaction_context(last_user_message, last_agent_message);
            let library = LocalReactionLibrary::default();
            Some(
                library
                    .select(category, strategy.local_preference)
                    .to_string(),
            )
        }
        BuddyReactionMode::AiOnly => generate_buddy_reaction_ai(
            session,
            turn_context,
            soul,
            last_user_message,
            last_agent_message,
        )
        .await
        .ok()
        .flatten(),
        BuddyReactionMode::Hybrid => {
            // Check if this is a critical interaction that should force AI
            if strategy.critical_scenarios_use_ai
                && is_critical_interaction(last_user_message, last_agent_message)
            {
                return generate_buddy_reaction_ai(
                    session,
                    turn_context,
                    soul,
                    last_user_message,
                    last_agent_message,
                )
                .await
                .ok()
                .flatten();
            }

            let agent_len = last_agent_message.map(str::len).unwrap_or(0);
            // Short replies use local presets
            if agent_len < strategy.min_reply_length {
                let category = classify_reaction_context(last_user_message, last_agent_message);
                let library = LocalReactionLibrary::default();
                return Some(
                    library
                        .select(category, strategy.local_preference)
                        .to_string(),
                );
            }
            // Probability-based AI usage
            let rand = (current_time_ms() as f64) / (u64::MAX as f64);
            if rand < strategy.ai_probability {
                generate_buddy_reaction_ai(
                    session,
                    turn_context,
                    soul,
                    last_user_message,
                    last_agent_message,
                )
                .await
                .ok()
                .flatten()
            } else {
                let category = classify_reaction_context(last_user_message, last_agent_message);
                let library = LocalReactionLibrary::default();
                Some(
                    library
                        .select(category, strategy.local_preference)
                        .to_string(),
                )
            }
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

# 本 fork 全量使用指南（新增能力）

本文面向已经了解/使用过上游 `openai/codex` 的用户，系统性介绍本 fork 新增或强化的能力，并给出「如何开启、如何使用」的中文示例。

> 约定：以下所有命令默认你已经安装好本仓库构建的 `codex`，并能正常登录至少一个基础模型（例如 OpenAI）。

目录：

1. 多模型 / 自定义 Provider / Anthropic
2. `codex serve` Web UI
3. Agent Teams 多 Agent 协作
4. Hooks 生命周期钩子（含技能内 hooks）
5. 周期任务与 `/loop` Slash 命令
6. GitHub Webhook 与 Outcome-first Overlay
7. 计划模式与退出确认等行为增强（简要）

---

## 1. 多模型 / 自定义 Provider / Anthropic

目标：在保持原有 OpenAI 模型可用的前提下，为某些任务切换到 Anthropic（或者自定义 provider）。

### 1.1 在 config.toml 中配置 Anthropic provider

编辑 `~/.codex/config.toml`，加入：

```toml
[model_providers.anthropic]
name = "Anthropic"
base_url = "https://api.anthropic.com"
env_key = "ANTHROPIC_API_KEY"
wire_api = "anthropic"

model_provider = "anthropic"
model = "claude-3-5-sonnet-20241022" # 示例，可按实际可用型号调整
```

Shell 中设置环境变量：

```bash
export ANTHROPIC_API_KEY="你的密钥"
```

启动 CLI/TUI 即会使用 Anthropic 作为默认 provider。如果只想在某些角色或场景下使用 Anthropic，可以：

- 保持全局默认仍为 OpenAI；
- 在 `~/.codex/agents/<role>.toml` 里单独指定：

```toml
model_provider = "anthropic"
model = "claude-3-5-sonnet-20241022"
```

然后在会话中选择对应 Agent 角色即可。

### 1.3 常见问题

- **Q: 我能混用多个 provider 吗？**
  - 可以。全局默认用一个 provider，某些 Agent 角色再显式切换到另一个 provider。
- **Q: 如何确认当前会话用的是哪个 provider？**
  - 查看会话初始化日志，或在 config 里显式写清 `model_provider` 与 `model`，避免隐式默认。

---

## 2. 使用 `codex serve` Web UI

目标：在本机浏览器里用 Codex，支持多会话、工具审批和终端。

### 2.1 启动 Web UI

```bash
codex serve
```

启动后终端会打印类似：

```text
Codex Web UI running at http://127.0.0.1:3847?token=xxxx...
```

直接复制该 URL 在浏览器打开即可。

默认行为：

- 仅绑定 `127.0.0.1`，只允许本机访问；
- 自动生成随机 token，所有请求都必须带上 token（URL 中已经包含）。

常用参数：

```bash
codex serve --host 127.0.0.1 --port 3847 --no-open
```

> 不推荐在无防护环境下使用 `--host 0.0.0.0`，如果必须使用，请确保网络和系统本身已做好访问控制。

### 2.2 Web UI 中的核心能力

- 左侧会话列表：新建 / 切换 / 重命名 / 归档会话；
- 聊天窗口：与 CLI/TUI 一致的对话流，包括工具调用与输出、子 Agent 事件等；
- 工具审批面板：对于 `shell` / `exec` 等敏感工具，可在 Web UI 中点选批准/拒绝；
- 终端面板：通过 WebSocket 与本机 PTY 连接，可以在浏览器中看到命令执行结果；
- 文件/Git 视图：通过 serve 的 HTTP API 获取文件内容、diff、Git 状态。

### 2.3 典型使用场景

- 本机跑 Codex，iPad/手机浏览器远程接入（建议依旧通过隧道或 VPN 暴露，务必注意安全）；
- 一边用 Web UI 做交互，一边让 TUI 只负责某些脚本化操作；
- 用浏览器窗口长期挂载一个“项目控制台”会话，便于回看工具调用历史。

---

## 3. Agent Teams 多 Agent 协作

目标：在同一个任务中，让多个专职子 Agent 并行工作，例如：规划→实现→评审。

这一套能力主要通过工具调用暴露（详见 `docs/agent-teams.md`），在 CLI/TUI 或 Web UI 中都可以调用。

### 3.1 创建一个简单的 Team

在会话中（例如 TUI）输入业务描述后，可以让模型调用 `spawn_team`。你也可以直接手动给出约定格式的调用需求，例如：

```text
请创建一个包含三个成员的团队：
- planner：负责拆解需求、输出实施计划
- implementer：负责按计划修改代码
- reviewer：负责审查实现是否符合需求
```

模型通常会自动生成 `spawn_team` 调用。Team 创建后，你可以：

- 用 `wait_team` 等待某个模式（全部完成 / 任意完成）；
- 用 `team_task_list` 查看任务列表；
- 用 `team_task_claim_next` 让某个成员领取下一条任务；
- 用 `team_message` 给某个成员单独发消息；
- 用 `team_task_complete` 标记任务完成。

Team 的持久化数据会自动保存在 `$CODEX_HOME/teams/<team_id>` 与 `$CODEX_HOME/tasks/<team_id>` 下，一般无需手动管理。

### 3.2 典型使用模式

- 小型任务：仍然使用单 Agent（默认模式），无需 Team；
- 中/大型任务：
  - 先由主 Agent 通过 `spawn_team` 建立团队；
  - 用 `planner` 生成详细计划；
  - `implementer` + `worker` 按计划拆分多条任务并行执行；
  - `reviewer` 审查关键变更、生成风控提醒和 TODO。

### 3.3 与上游单 Agent 模式的区别

- 上游：`spawn_agent` 主要作为一个“偶尔开个小号”的机制，状态管理相对轻量；
- 本 fork：
  - 把 team 概念一等化，有独立的持久化目录和任务表；
  - 提供专门的任务/消息工具，支持更复杂的多 Agent 编排；
  - 为后续 swarm/control-plane 设计打基础（见 `docs/plans/2026-03-06-codex-swarm-architecture.md`）。

---

## 4. Hooks 生命周期钩子

目标：在关键动作前后插入自定义逻辑，例如：

- 拦截高风险工具调用；
- 对特定目录的改动强制进行 code review；
- 记录敏感操作审计日志。

### 4.1 在 config.toml 开启一个简单的 pre_tool_use hook

示例：对 `shell`/`exec` 工具调用做一次命令级检查。

在 `~/.codex/config.toml` 中添加：

```toml
[hooks]

[[hooks.pre_tool_use]]
command = ["python3", "/Users/me/.codex/hooks/check_tool.py"]
timeout = 5
once = true

[hooks.pre_tool_use.matcher]
tool_name_regex = "^(shell|exec)$"
```

`check_tool.py` 将在每次匹配的工具调用前收到一份 JSON 输入（stdin），可选择：

- 正常退出（code 0）：继续执行工具；
- 用 exit code 2 退出：阻断本次调用（stderr 作为阻断原因）；
- 输出 JSON 到 stdout，Codex 会尝试解析并将其中的 `systemMessage` / `additionalContext` 注入到下一轮对话中。

更详细的字段说明见：

- `docs/hooks.md`（事件 payload 和输出格式）；
- `docs/config.md` 中的 Hooks 部分（配置项说明）。

### 4.2 常见用法示例

- 只允许白名单命令：在 `check_tool.py` 中检查 `tool_input` 中的命令是否在白名单，否则 exit 2；
- 记录审计：将每次工具调用的 payload 追加写入本地日志文件或发送到审计系统；
- 多 Agent 风控：对 `subagent_start` / `task_completed` 等事件挂 hook，记录各子 Agent 的关键操作。

### 4.3 在技能（Skill）里挂 hook

除了全局/项目级 config，本 fork 支持在 `SKILL.md` 的 YAML frontmatter 中定义「技能作用期间生效」的 hooks：

```yaml
---
name: ralph-wiggum
description: Block stop until the user promises
hooks:
  Stop:
    - hooks:
        - type: command
          command: "python3 .claude/hooks/ralph-wiggum-stop-hook.py"
---
```

特点：

- 只在该技能激活的「那一回合」生效，回合结束自动卸载；
- 适合把某些风控、审计逻辑“封装进技能”，而不是全局硬绑；
- 与上面的 config.toml hooks 可以同时存在，触发时会按文档中的去重和并行规则执行。

更多详细字段和事件说明见 `docs/hooks.md`。

---

## 5. `/loop` 与周期任务

目标：像使用 cron 一样，让 Codex 定期执行某个诊断或检查任务。

### 5.1 确认已启用

在 `~/.codex/config.toml` 中确认（或添加）：

```toml
disable_cron = false
```

若设置为 `true`，周期任务工具会禁用，TUI 中不会提供 `/loop` 命令。

### 5.2 使用 `/loop`

在 TUI 或 Web UI 中直接输入：

```text
/loop 15m check build status
```

或使用自然语言：

```text
/loop review PR every 2 hours
```

行为：

- `/loop` 会被改写成一条普通用户消息，引导模型调用内部 scheduled-task tools；
- 成功后会创建一个周期任务，按照设定间隔自动执行；
- 你可以在后续会话中让模型列出 / 修改 / 取消这些任务（取决于当前工具集的实现）。

### 5.3 与上游的差异

- 上游 Codex 没有 `/loop` 这条额外的 Slash 命令；
- 本 fork 内置了基于 `disable_cron` 的调度工具与 UI 集成，让“定时跑任务”成为一等能力。

---

## 6. GitHub Webhook 与 Outcome-first Overlay（概览）

目标：让 `codex github` 更易于配置，逐步过渡到“以结果为中心”的自动化流程。

### 6.1 基本配置

在 `~/.codex/config.toml` 中添加：

```toml
[github_webhook]
enabled = true
listen = "127.0.0.1:8787"
webhook_secret_env = "GITHUB_WEBHOOK_SECRET"
github_token_env = "GITHUB_TOKEN"
github_app_id_env = "GITHUB_APP_ID"
github_app_private_key_env = "GITHUB_APP_PRIVATE_KEY"
auth_mode = "auto"
min_permission = "read"
allow_repos = ["owner/repo"]
command_prefix = "/codex"
delivery_ttl_days = 7
repo_ttl_days = 0
sources = ["repo", "organization", "github-app"]

[github_webhook.events]
issue_comment = true
issues = true
pull_request = true
pull_request_review = true
pull_request_review_comment = true
push = true
```

之后运行：

```bash
codex github
```

并在 GitHub 仓库中配置相应 webhook 和命令前缀（例如评论 `/codex fix`）即可。

### 6.2 Outcome-first Overlay（高级用法）

`docs/github-outcome-first-overlay.md` 描述了一种“先澄清需求，再执行，再给出证明”的 GitHub 编排模式。当前实现仍以原生 `codex github` 路径为主：

- 如果你只需要“收到 webhook 就跑 Codex 并回复”，保持默认配置即可；
- 如果需要 overlay 式的更复杂路由，请按文档说明在会话中显式启用，并严格遵守里程碑式输出约定（PRD、proof 等）。

---

## 7. 计划模式与退出确认等行为增强（简要）

本 fork 还对一些行为做了增强，虽然不完全是“新功能”，但使用体验与上游有差异，简单列一下：

### 7.1 Plan 模式与配置

- 增加了 `plan_mode_reasoning_effort` 配置项，用来单独控制 Plan 模式下的默认推理强度：

```toml
plan_mode_reasoning_effort = "medium" # or "low" / "high" / "none"
```

- 当你在会话中显式进入 Plan 模式时，Codex 会根据这个值决定是否做更深入推理，而不会简单沿用普通对话的默认。

### 7.2 退出确认提示

- CLI/TUI 在 Ctrl+C / Ctrl+D 退出时，引入了「双击确认」行为：
  - 第一次 Ctrl+C：提示 `ctrl + c again to quit`；
  - 第二次才真正退出；
- 目的是减少误触 Ctrl+C 时直接丢失上下文的情况。

---

## 8. 推荐阅读顺序

如果你是从上游 Codex 迁移过来的，可以按以下顺序补齐本 fork 的使用心智：

1. `docs/fork-vs-upstream-codex.md`：整体差异和新增模块；
2. `docs/config.md`：多 provider、Hooks、scheduled tasks、GitHub webhook 等配置；
3. 本文：具体使用场景与命令示例；
4. `docs/agent-teams.md`：多 Agent 协作的细节；
5. `docs/prd-codex-serve.md`：Web UI 与 serve crate 设计细节；
6. `docs/github-outcome-first-overlay.md`：未来 GitHub 编排方向（可选、偏高级）。

如需更细化的中文教程（例如只讲 Agent Teams 或只讲 `codex serve`），可以单独告诉我你最常见的使用场景，我可以基于本文再拆出更详细的分主题文档。 

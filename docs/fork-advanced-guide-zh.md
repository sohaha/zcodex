# 本 fork 进阶实战（中文）

本文提供本 fork 新增能力的可复制模板，适合已经完成基础上手、准备进入团队实战的用户。

配套阅读：

- 快速上手：`docs/fork-quickstart-zh.md`
- 全量说明：`docs/fork-usage-guide-zh.md`
- 差异总览：`docs/fork-vs-upstream-codex.md`

---

## 1. 多 Provider 与分角色模型模板

目标：全局默认使用 OpenAI，特定角色切到 Anthropic。

### 1.1 `~/.codex/config.toml`

```toml
# 全局默认（示例）
model_provider = "openai"
model = "gpt-5"

[model_providers.openai]
name = "OpenAI"
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
wire_api = "responses"

[model_providers.anthropic]
name = "Anthropic"
base_url = "https://api.anthropic.com"
env_key = "ANTHROPIC_API_KEY"
wire_api = "anthropic"
```

### 1.2 `~/.codex/agents/reviewer.toml`

```toml
model_provider = "anthropic"
model = "claude-3-5-sonnet-20241022"
```

说明：

- 常规实现任务继续走默认 provider；
- 风险审查或文档审查交给 reviewer 角色，自动切换模型。

---

## 2. `codex serve` 安全运行建议

推荐启动命令：

```bash
codex serve --host 127.0.0.1 --port 3847 --no-open
```

建议：

- 默认仅本机回环地址，不直接对公网暴露；
- 通过 URL token 登录，不要把包含 token 的链接发到公共频道；
- 若必须远程访问，优先通过 VPN / 零信任网关，而不是直接 `0.0.0.0` 暴露端口。

---

## 3. Agent Teams 可复用模板

### 3.1 推荐团队结构

- `planner`: 产出任务拆解与验收标准
- `implementer`: 负责编码与本地验证
- `reviewer`: 负责风险检查与回归清单

### 3.2 工具调用模板

`spawn_team` 请求体示例：

```json
{
  "team_id": "feature-x-team",
  "members": [
    {
      "name": "planner",
      "task": "拆解需求并给出可验证验收标准",
      "agent_type": "architect"
    },
    {
      "name": "implementer",
      "task": "按计划实现并跑最小验证",
      "agent_type": "develop",
      "worktree": true
    },
    {
      "name": "reviewer",
      "task": "审查边界条件和回归风险",
      "agent_type": "code-review",
      "background": true
    }
  ]
}
```

建议流程：

1. `spawn_team`
2. `wait_team`（`mode: "any"` 先收第一批结果）
3. `team_message` 补充要求
4. `team_task_complete`/`team_task_claim_next`
5. `wait_team`（`mode: "all"`）
6. `team_cleanup`

---

## 4. Hooks 生产可用模板

### 4.1 目标

对 `shell`/`exec` 做命令白名单检查，不符合则阻断。

### 4.2 `~/.codex/hooks/check_tool.py`

```python
#!/usr/bin/env python3
import json
import re
import sys

payload = json.load(sys.stdin)
tool_name = payload.get("tool_name", "")
tool_input = payload.get("tool_input", {})

if tool_name not in {"shell", "exec"}:
    print("{}")
    sys.exit(0)

cmd = ""
if isinstance(tool_input, dict):
    cmd = str(tool_input.get("command", ""))

allowed = [
    r"^git status$",
    r"^git diff",
    r"^rg ",
    r"^cargo test",
]

if any(re.match(p, cmd) for p in allowed):
    print("{}")
    sys.exit(0)

sys.stderr.write(f"blocked by hook: disallowed command: {cmd}\n")
sys.exit(2)
```

### 4.3 `~/.codex/config.toml` 中挂载

```toml
[hooks]

[[hooks.pre_tool_use]]
command = ["python3", "/Users/me/.codex/hooks/check_tool.py"]
timeout = 5

[hooks.pre_tool_use.matcher]
tool_name_regex = "^(shell|exec)$"
```

效果：

- 匹配到非白名单命令时，工具调用被阻断；
- 阻断原因显示在会话中，行为可观察、可调试。

---

## 5. `/loop` 周期任务实战模板

前置：`disable_cron = false`。

常用示例：

```text
/loop 10m check ci status and summarize failures
/loop 2h review stale PRs and suggest next actions
/loop 1d summarize repo risk hotspots changed in last 24h
```

建议：

- 周期间隔不要过短，避免噪声；
- 让提示词输出固定格式（例如三段式：状态 / 风险 / 建议）；
- 给周期任务附带明确停止条件（例如“连续 3 次成功后提醒我关闭此任务”）。

---

## 6. GitHub Webhook 配置模板

`~/.codex/config.toml`：

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
sources = ["repo", "organization", "github-app"]

[github_webhook.events]
issue_comment = true
issues = true
pull_request = true
pull_request_review = true
pull_request_review_comment = true
push = true
```

启动：

```bash
codex github
```

触发示例（GitHub 评论）：

```text
/codex fix flaky test in module x and open a PR
```

---

## 7. 排障清单

- `codex serve` 打不开：检查端口占用、URL 是否带 token、是否被本地防火墙拦截。
- Anthropic 不生效：检查 `ANTHROPIC_API_KEY` 是否在当前 shell 生效，`model_provider` 是否被 profile 覆盖。
- Team 卡住：用 `wait_team` 看状态，再用 `team_message` 定向催促成员；最后 `close_team` + `team_cleanup` 回收。
- Hook 没触发：检查 matcher 是否匹配事件字符串，检查脚本路径和可执行权限。
- `/loop` 不可见：检查 `disable_cron` 是否设成了 `true` 或被 profile 覆盖。

---

## 8. 迁移建议（上游 -> 本 fork）

建议按这个顺序渐进启用，而不是一次性全开：

1. 先保持上游习惯，仅多用 `codex serve`；
2. 再启用一个额外 provider（例如 Anthropic）；
3. 接着把一个中型任务改成 Agent Team；
4. 最后再引入 Hooks 与 GitHub webhook 自动化。

这样更容易定位问题，也更利于团队内部推广。

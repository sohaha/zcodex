# 本 fork 快速上手（中文）

这是一份 5~10 分钟可走完的快速上手，面向从上游 `openai/codex` 迁移过来的用户。

如果你想看完整能力，请继续阅读：

- `docs/fork-docs-index-zh.md`
- `docs/fork-usage-guide-zh.md`
- `docs/fork-advanced-guide-zh.md`
- `docs/fork-enterprise-guide-zh.md`

---

## 1) 安装并验证

```bash
codex --version
```

能输出版本号即可。

---

## 2) 先跑 Web UI（本 fork 核心新增）

```bash
codex serve
```

终端会打印：

```text
Codex Web UI running at http://127.0.0.1:xxxx?token=...
```

浏览器打开这个 URL，就能直接使用会话管理、工具审批、终端面板等能力。

---

## 3) 启用 Anthropic（可选）

编辑 `~/.codex/config.toml`：

```toml
[model_providers.anthropic]
name = "Anthropic"
base_url = "https://api.anthropic.com"
env_key = "ANTHROPIC_API_KEY"
wire_api = "anthropic"

model_provider = "anthropic"
model = "claude-3-5-sonnet-20241022"
```

设置环境变量：

```bash
export ANTHROPIC_API_KEY="你的密钥"
```

重启 Codex 后生效。

---

## 4) 试一次多 Agent Team（可选）

在会话里输入：

```text
请创建一个三人团队：planner 做计划，implementer 写代码，reviewer 做审查。
```

模型通常会自动调用 `spawn_team`。之后可继续让它调用：

- `wait_team`
- `team_task_list`
- `team_task_complete`

这就是本 fork 的 Agent Teams 基本流程。

---

## 5) 开启 `/loop` 定时任务（可选）

确认 `~/.codex/config.toml`：

```toml
disable_cron = false
```

然后在会话里输入：

```text
/loop 15m check build status
```

会创建一个每 15 分钟执行一次的周期任务。

---

## 6) 先用哪份文档

- 想快速跑通：当前文档
- 想看完整能力与场景：`docs/fork-usage-guide-zh.md`
- 想直接抄配置模板（Hooks/GitHub Webhook/多角色模型）：`docs/fork-advanced-guide-zh.md`
- 想按团队落地与安全治理实施：`docs/fork-enterprise-guide-zh.md`

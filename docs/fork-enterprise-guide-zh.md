# 本 fork 企业团队落地指南（中文）

本文给出一个可落地的团队方案，目标是：在引入本 fork 新能力（多模型、Agent Teams、Hooks、`codex serve`、GitHub webhook）时，尽量做到可控、可审计、可回滚。

配套文档：

- 中文索引：`docs/fork-docs-index-zh.md`
- 快速上手：`docs/fork-quickstart-zh.md`
- 进阶模板：`docs/fork-advanced-guide-zh.md`
- 全量教程：`docs/fork-usage-guide-zh.md`

---

## 1. 落地范围建议

先试点，再全量：

1. 阶段 A（1~2 周）：只启用 `codex serve` + 单 Agent；
2. 阶段 B（2~4 周）：引入多 provider 与 `/loop`；
3. 阶段 C（4~6 周）：引入 Agent Teams 与 Hooks；
4. 阶段 D（按需）：引入 `codex github` webhook 自动化。

这样做的收益：

- 能快速定位故障来源；
- 组织学习成本更低；
- 风险面和权限面逐步扩大，而不是一次性放开。

---

## 2. 角色与职责

建议最小角色：

- **平台管理员**：
  - 维护 `~/.codex/config.toml` 基线模板；
  - 维护 Hook 脚本仓库与发布流程；
  - 管理 provider 密钥注入方式。
- **项目负责人**：
  - 决定项目级 `.codex/config.toml` 是否启用；
  - 决定是否允许 Agent Teams 与 webhook 自动化。
- **开发者**：
  - 按团队模板启动会话；
  - 对工具审批负责；
  - 遵守审计与提交规范。

---

## 3. 配置基线（建议）

### 3.1 用户级（`~/.codex/config.toml`）

建议至少明确以下项：

```toml
# 模型默认
model_provider = "openai"
model = "gpt-5"

# 保持 cron 显式开启（如团队需要 /loop）
disable_cron = false

# GitHub webhook（按需）
[github_webhook]
enabled = false
listen = "127.0.0.1:8787"
command_prefix = "/codex"
```

### 3.2 项目级（`./.codex/config.toml`）

建议只放“项目特有”配置，例如：

- 项目 Hooks；
- 项目允许的工具模式；
- 项目默认 profile。

不要在项目配置里写密钥；密钥统一走环境变量或集中密钥管理。

---

## 4. 权限与工具审批策略

建议按环境分层：

- **本地开发**：`on-request` 或 `on-failure`；
- **CI/自动化执行**：严格白名单 + Hook 阻断；
- **高风险仓库**：默认 deny，再按场景放行。

最小策略：

1. 对 `shell`/`exec` 增加 `pre_tool_use` Hook；
2. 对敏感命令（删除、重置、外网写操作）默认阻断；
3. 阻断信息必须可见（stderr 返回阻断理由）；
4. 规则变更必须走版本管理，不允许本地手改长期漂移。

---

## 5. 审计与可追溯性

建议审计三层数据：

- **会话层**：thread/transcript（谁在什么上下文执行了什么）；
- **工具层**：hook payload（请求参数、审批结果、阻断原因）；
- **代码层**：Git commit + PR 评论链路。

实践建议：

- Hook 脚本把关键事件写入 JSONL（本地或集中日志）；
- 对 `task_completed`、`subagent_start` 等多 Agent 事件单独记录；
- Git commit 信息保持可检索格式（如 Conventional Commits）。

---

## 6. Agent Teams 团队作业规范

推荐默认三角色：

- `planner`：只产计划与验收标准；
- `implementer`：负责改动与本地验证；
- `reviewer`：负责风险与回归检查。

流程建议：

1. `spawn_team` 建队；
2. `planner` 输出验收标准（可执行、可验证）；
3. `implementer` 在独立 worktree 执行；
4. `reviewer` 生成风险清单；
5. 统一由主线程决定合并/回滚；
6. `team_cleanup` 回收状态目录。

禁止项建议：

- 不允许 team 嵌套 team；
- 不允许绕过审批直接执行高风险命令；
- 不允许 reviewer 直接改代码（保持职责清晰）。

---

## 7. `codex serve` 部署建议

最低安全建议：

- 默认 `127.0.0.1` 绑定；
- 使用随机 token；
- 不把 token URL 泄露到公开渠道；
- 远程访问通过 VPN/跳板，不直接暴露公网端口。

稳定性建议：

- 固定端口（便于反向代理与书签管理）；
- 会话归档策略（定期归档长期会话）；
- 浏览器端与 CLI/TUI 使用同一项目目录，减少上下文漂移。

---

## 8. GitHub webhook 试点建议

先在单仓试点：

1. `allow_repos` 只放 1~2 个仓库；
2. 只开启 `issue_comment` 与 `pull_request_review_comment`；
3. 使用明确 `command_prefix`（如 `/codex`）；
4. 明确“谁有权限触发自动执行”。

稳定后再扩：

- 开 `issues`、`pull_request`、`push` 事件；
- 引入 outcome-first overlay 的流程化输出（按 `docs/github-outcome-first-overlay.md`）。

---

## 9. 故障处理与回滚

常见回滚手段：

- 关闭 `github_webhook.enabled`；
- 关闭 `disable_cron = true`（禁用 `/loop`）；
- 注释掉高风险 hooks；
- team 任务中断后执行 `close_team` + `team_cleanup`。

应急 SOP 建议：

1. 先冻结自动化入口（webhook、cron）；
2. 保留日志与现场；
3. 仅允许人工审批模式继续；
4. 修复后小流量恢复。

---

## 10. 上线检查清单

- [ ] provider 配置与密钥注入路径已验证  
- [ ] `codex serve` 仅内网/本机访问，token 生效  
- [ ] `pre_tool_use` Hook 阻断策略已演练  
- [ ] Agent Teams 三角色流程已跑通  
- [ ] `/loop` 任务有停用/清理流程  
- [ ] webhook 事件范围、权限范围、仓库白名单已确认  
- [ ] 审计日志可检索且可追溯到 commit/PR  

## 11. 一键配置巡检脚本

仓库内置了一个运维巡检脚本：`scripts/check-fork-config.py`，用于检查关键配置项是否齐全。

默认检查（读取 `~/.codex/config.toml`）：

```bash
python3 scripts/check-fork-config.py
```

严格模式（有 WARN 即返回非 0，适合 CI）：

```bash
python3 scripts/check-fork-config.py --strict
```

指定配置文件：

```bash
python3 scripts/check-fork-config.py --config /path/to/config.toml --strict
```

脚本会检查：

- `model_provider` / `model` / `[model_providers.*]`
- `disable_cron`
- `[hooks].pre_tool_use`
- `[github_webhook]` 及关键字段
- 被配置引用的环境变量是否在当前 shell 可见

如果你希望，我可以基于你当前团队的真实流程（GitHub/GitLab、单仓/多仓、是否有 VPN/堡垒机）再给一份“可直接执行”的企业配置模板。 

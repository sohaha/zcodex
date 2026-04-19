# 2026-04-19 CTF 上游同步技能要同时钉住上游参考面和本地保留面

## 背景

- 本仓的 `codex ctf` 子命令最初就是参考 `ryfineZ/codex-session-patcher` 落地的。
- 但本地实现只吸收了其中的 Codex 相关工作流：
  - `codex ctf` 启动时注入 CTF 指令
  - rollout refusal 清理
  - CTF 会话的 clean-then-resume
- upstream 仓库本身还带了 Web UI、多平台格式、安装器和 AI 改写。如果没有 repo 专用 skill，后续“同步上游”很容易把这些非目标能力也当成默认范围。

## 结论

- 针对长期重复的上游同步任务，skill 不能只写“去看 upstream repo”，必须把：
  - 首选上游事实源
  - 本地正常修改面
  - 默认保留的分叉边界
  一次性写死。
- 对 `codex-session-patcher` 这类多能力仓库，默认上游参考面至少要拆成两层：
  - 总览事实源：`README.md`、`core/formats.py`
  - 按概念补读：`detector.py`、`patcher.py`、`ctf_config/templates.py`、`ctf_config/installer.py`
- 本地保留面也要显式列出，否则后续同步时容易把“当前架构选择”误判成“落后于 upstream 的缺口”。本次确认需要默认保留的有：
  - 原生 `codex ctf` 子命令，而不是 Python 安装器
  - `ctf_config.rs` 的 base-instructions 注入
  - `rollout/src/patch.rs` 的显式 clean
  - `tui/src/ctf_resume.rs` 的显式 clean-then-resume
- `STATE.md` 和 checklist 应与 skill 同时创建，不要等第一次真实同步时再临时补；否则第一次执行时容易把“怎么同步”与“这次同步到了哪”混在一起。

## 落地做法

- 新增了 repo 专用技能 `.codex/skills/sync-codex-session-patcher/`。
- 技能正文固定了 upstream ref、默认 selective sync 规则、默认忽略的 upstream 范围和本地主修改面。
- 同时补了 `STATE.md`、`references/checklist.md` 和 `agents/openai.yaml`，让后续同步既有状态锚点，也有稳定触发入口。

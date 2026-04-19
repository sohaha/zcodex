# 2026-04-09 zoffsec resume clean-then-resume 反思

## 背景
- `codex zoffsec clean` 已经具备对 rollout 做 refusal/reasoning 清理的能力，但 `zoffsec` 命令族还缺少“恢复前自动 clean”的闭环。
- 需求同时要求复用现有 resume 选择器体验，不能为了 clean 另起一套会话选择 UI。

## 本轮有效做法
- 在 CLI 层新增 `codex zoffsec resume` / `codex zoffsec r`，并在进入恢复流程前输出显式提示，让“会执行 clean”对用户可见。
- 不把 clean 逻辑塞进主 `codex resume`，而是通过 `TuiCli.resume_zoffsec_clean` 这个内部标记，把行为限定在 `zoffsec resume` 路径。
- 在 `tui/src/zoffsec_resume.rs` 新增独立 hook：等现有 resume picker / `--last` / `<session_id>` 完成目标选择后，再判断 session meta 是否带有 zoffsec marker；只有命中 zoffsec 会话时才调用 `clean_zoffsec_rollout()`。
- 这样既保留了现有 resume 选择与 cwd 解析链路，也避免在 `tui/src/lib.rs` 大文件里继续堆叠清理细节。

## 关键收益
- 行为边界清晰：只有 `zoffsec resume` 才触发 clean-then-resume，普通 `codex resume` 完全不变。
- 选择器复用完整：`--last`、按 ID/名称恢复、默认 picker 三条路径都复用现有 TUI 逻辑。
- 清理逻辑集中：zoffsec 检测、路径回退与 clean 调用都在独立模块，可单测。

## 踩坑
- 当前仓库存在与本任务无关的脏改动，会直接阻断 issue 约定的验证命令：
  - `codex-rs/core/src/tools/rewrite/engine.rs`
  - `codex-rs/core/src/tools/rewrite/auto_tldr.rs`
  - `codex-rs/core/src/tools/rewrite/read_gate.rs`
  - `codex-rs/cli/src/tldr_cmd.rs`
- 这类阻断不应该顺手修进当前任务；更稳妥的做法是把 issue 标成 `blocked`，附上失败命令和具体文件/行号，等待对应任务先收敛。

## 后续建议
- 以后如果还要引入“resume 前附加一步处理”的变体，优先沿用“CLI 显式子命令 + TUI 内部标记 + 选择后 hook”的模式，不要污染通用 resume 主路径。
- 若需要把 pre-resume hook 做成可复用框架，再考虑抽象；当前只有 zoffsec 一条路径时，保持专用模块更容易控边界。

## 收尾补记
- `format_exit_messages()` 现在会先输出 protocol `FinalOutput` 的英文 token usage，再输出中文本地化 usage；CLI 断言需要按双语输出更新，不能再假设零 usage 时返回空列表。
- Cadence issue 文件里的 `validate_status` / `regress_status` 必须使用 `passed`，不能写成 `pass`，否则 `cadence_validate.js` 会在回归阶段直接失败。

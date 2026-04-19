# 2026-04-19 pending_input 验证链要先打通 tests/all 编译面，并区分路由意图与 ztok 输出格式

## 背景

- 本轮任务先在 `codex-rs/core/tests/suite/pending_input.rs` 做汉化，并补了一条中性 `pending_input` 保留原始 grep 指令的回归。
- 目标验证命令是 `cargo test -p codex-core --test all suite::pending_input -- --nocapture`。

## 实际踩坑

### 1. `suite::pending_input` 的验证不只受 `pending_input.rs` 自己影响

- `--test all` 会先编译整个 `core/tests/all.rs` 聚合测试目标。
- 所以即使只想验证 `suite::pending_input`，也会先被别的 suite 的测试层 API 漂移挡住，例如：
  - `ModelProviderInfo` 从 `codex_core` 根导出迁到 `codex_model_provider_info`
  - `ProjectConfig` / `ConfigToml` 私有导出收口到 `codex_config::config_toml`
  - `ModelProviderInfo.name` 改成 `Option<String>`
  - `ModelInfo` 新增 `skip_reasoning_popup`

### 2. grep 指令回归不能把“原始 stdout 形状”当成唯一事实

- 新增回归最初把断言写成“`function_call_output` 必须包含原始 `rg` 的逐行输出，且不能出现 `ztok:`”。
- 但当前运行面会通过 shell rewrite 把 `rg` 包进 `codex ztok grep`，因此工具输出前缀和摘要形状会变化，仍然可能是正确行为。
- 真正需要验证的是：
  - follow-up request 里是否保留了初始“不要 ztldr，用 ripgrep”的用户意图
  - 工具输出是否仍然是 grep 结果，而不是被改路由成 `ztldr` 或其他搜索面

## 结论

1. 以后在 `codex-core` 验证单个 `suite::...` 时，只要命令形态还是 `--test all`，就要预期先修聚合测试的编译面，再谈目标用例本身。
2. 对 shell/search 路由相关回归，优先断言“请求中的意图与最终工具种类”而不是写死某个代理前 stdout 形状。
3. dirty worktree 下接收 snapshot 后，要顺手清掉 `.snap.new` 的暂存残留；否则很容易把已接受 snapshot 和中间产物一起留在提交边界里。

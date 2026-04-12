# 2026-04-12 codex-core MCP 测试的 nextest 超时分流

## 背景
- 这轮在 `codex-core` 补了多处 `wait_for_mcp_tools(...)`，用来等插件 / RMCP / truncation / search 相关测试里的 MCP tool 真正完成注册。
- 跑 `RUSTC_WRAPPER= just core-test-fast` 时，目标用例本身并没有断言失败，但 `nextest` 的单测 30 秒超时先把它们判成失败，集中出现在 `plugins`、`rmcp_client`、`search_tool`、`truncation` 这类需要先等 MCP tool ready 的测试。

## 这轮有效做法
- 不要把这类 `TIMEOUT [30s]` 直接当成逻辑回归；先看是否只是 MCP 启动路径比 `nextest` 的默认单测时限更慢。
- 对超时用例改用定向 `cargo test -p codex-core --test all suite::... -- --exact --nocapture` 复跑，确认真实结果。
- 如果定向 `cargo test` 通过，而 `nextest` 仍超时，就把它归类为 runner 时限问题，而不是继续在业务逻辑层盲改。

## 关键收益
- 避免把“测试 harness 的时间预算不足”误判成 MCP 注册、tool list 或 truncation 逻辑有 bug。
- 在 dirty worktree 里能快速验证当前补丁是否正确，而不被全量 `nextest` 的单 case timeout 误导。

## 后续建议
- 以后遇到 `codex-core` 里依赖 MCP tool readiness 的测试只在 `nextest` 下超时，优先用定向 `cargo test --exact` 复核一次。
- 若后续这类用例持续增多，再考虑统一给相关测试单独的 nextest timeout 策略，而不是在每个测试里继续叠加更长的 sleep。

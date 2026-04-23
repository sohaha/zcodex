## 背景

给 `multi_agent_v2` 相关的“开发中功能” warning 做汉化时，表面入口是 CLI 启动提示，但同一类文案同时存在于 `features` 元数据生成、CLI 直出、core 集成断言、CLI 集成测试，以及 TUI 里实验功能切换后的错误/历史消息。

## 反思

- 实验性或开发中功能的汉化不能只改一个入口。至少要同时检查 `codex-rs/features/src/lib.rs`、`codex-rs/cli/src/main.rs`、`codex-rs/core/tests/suite/unstable_features_warning.rs`、`codex-rs/cli/tests/features.rs` 和 `codex-rs/tui/src/app.rs`，否则很容易留下“源码已汉化，测试或另一路 UI 仍是英文”的半汉化状态。
- `features` crate 是这类文案的源头之一，功能菜单名、说明和 warning 经常在这里集中定义；如果 TUI 或 CLI 的表现异常，先核对 feature metadata，而不是只盯视图层。
- 做这条链路的验证时，`codex-rs/features/src/tests.rs` 里陈旧的 feature 断言可能会先把测试打爆，导致误判为本次汉化回归。像已不存在的 `WorkspaceDependencies` 这类失效断言，需要先清掉，再判断本次改动是否真的有问题。

## 结论

以后处理实验功能/开发中功能相关汉化时，默认按“源头文案 + CLI 输出 + core 事件断言 + TUI 提示 + 相关集成测试”整链路收口，并把失效 feature 测试视为优先清理项。

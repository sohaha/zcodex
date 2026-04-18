# 反思：provider 默认模型必须在 provider 选择之后解析

## 背景
用户要求 `model_providers.<id>.model` 的优先级高于全局 `model`，低于命令行 `--model`，并且确认通过 `-P` / `--model-provider` 切换 provider 时也要生效。

## 结论
- `Config::load_config_with_layer_stack` 里不能在确定最终 provider 之前就解析 `model`。正确顺序是先算出 `model_provider_id` 和对应 `model_provider`，再把 `model_provider.model` 纳入 `Config.model` 的合并链。
- 这条链路的优先级应固定为：命令行 `model` 覆盖 > 已选 provider 的 `model` > profile `model` > 全局 `model`。
- `-P` / `--model-provider` 是否生效，不需要额外特殊分支；关键是 CLI provider override 必须先进入 `model_provider_id` 的选择，再由选中 provider 的 `model` 参与后续解析。
- 文档需要明确两件事：provider 自带 `model` 会覆盖全局 `model`，但不会覆盖命令行 `--model`；而且这条规则同样适用于通过 `-P` / `--model-provider` 切换到的 provider。

## 验证边界
- `cargo check -p codex-core --lib` 可用于验证生产代码链路是否通过编译。
- 当前仓库的 `just fmt` / `cargo fmt --all` 会被无关文件 `codex-rs/core/tests/suite/shell_command.rs` 的语法错误阻塞，不能把格式化失败误判为本次改动引入。
- 当前仓库的 `cargo test -p codex-core ...` 还会被大量既有 lib test 编译漂移阻塞；至少应复查编译日志，确认新增或修改的测试行没有引入新的错误，再把“测试未跑通”的原因写清楚。

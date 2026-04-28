# CLI provider 空值选择要保留 `-c` 优先级

## 背景
- `codex -P` 原先在 clap 解析阶段直接报 “a value is required”，后续 TUI 启动逻辑没有机会触发 provider 选择。
- `-P <provider>` 既有语义是低优先级地注入 `model_provider=<provider>`，显式 `-c model_provider=...` 必须继续覆盖它。

## 经验
- 对带值 flag 增加“无值也合法”的能力时，不能只把 clap 字段改成 `num_args = 0..=1`；还要在运行时区分三种状态：未传、传了空值、传了实际值。
- 空值触发交互式选择时，不要通过高优先级 `ConfigOverrides.model_provider` 盲目覆盖配置；如果 CLI 原始 `-c` 里已经有 `model_provider=...`，应跳过选择器并保留 `-c` 优先级。
- 这类 CLI 入口回归需要同时覆盖内部 TUI parser 和顶层 `codex` parser，因为 `codex` 会 flatten TUI 参数，解析行为可能在外层提前失败。

## 验证建议
- 增加 `codex-tui` 层解析测试：未传 `-P` 为 `None`，`-P` 为空字符串请求，`-P ollama` 保留 provider id。
- 增加顶层 `codex-cli` 解析测试：`codex -P` 应能进入 interactive CLI，而不是被 clap 拦截。
- 增加优先级测试：`-c model_provider=...` 存在时，空 `-P` 不应触发手动选择覆盖。

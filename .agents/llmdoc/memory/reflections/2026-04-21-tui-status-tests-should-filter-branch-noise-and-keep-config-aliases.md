# TUI `/status` 测试要过滤 branch 噪音，改 `codex_home` 的测试要同步 `sqlite_home`，状态栏配置 id 改名要保留兼容别名

## 背景

这次收敛 `codex-tui` 回归时，最后一批失败不是核心逻辑坏掉，而是测试和兼容边界一起漂了：

- `/status` 相关测试直接 `try_recv()` 第一条事件，结果先读到了 `StatusLineBranchUpdated`
- 一个 thread memory mode 测试只改了 `codex_home`，却没同步 `sqlite_home`，导致 app-server 写入和断言读取落在两份不同的 state DB
- 状态栏配置从 `context-remaining-percent` 收敛到 `context-remaining` 后，旧配置键不再被识别
- rename prompt 不再预填当前线程名，导致按 Enter 时因为输入为空而没有提交事件

## 关键结论

### 1. `/status` 或 app 事件测试不要假设“第一条事件就是目标事件”

`StatusLineBranchUpdated` 属于启动或刷新期噪音事件。只要测试目标是 `/status` 渲染、rate-limit refresh、history replay 之类的功能，就应该在 helper 里先过滤这类 branch 更新，再断言真正目标事件。

否则：

- 测试会把正常异步事件顺序变化误报成回归
- 本质上是“测试取错消息”，不是“功能没发消息”

### 2. TUI / app-server 测试里手动改 `codex_home` 时，要一起改 `sqlite_home`

app-server 的 thread memory mode 和其他 state runtime 数据写入实际走 `config.sqlite_home`。如果测试只覆写 `codex_home`，后续再用新的 `codex_home` 路径初始化 `StateRuntime` 读取，就会读到另一份库。

结论：

- 只要测试显式改 `codex_home`
- 且断言依赖 sqlite state
- 就要同步把 `sqlite_home` 指到同一目录，或直接用 `config.sqlite_home` 读取

### 3. 状态栏 item id 改名时，要保留旧配置 alias

`StatusLineItem::ContextRemaining` 从 `context-remaining-percent` 收敛到 `context-remaining` 后，如果不保留 alias，老 `config.toml` 和相关测试都会静默失效，表现为该项被忽略而不是显式报错。

这里更稳的做法是：

- 新 id 作为 `to_string`
- 旧 id 作为 `serialize` alias 保持向后兼容

这样 UI、测试和已有配置文件都能平滑过渡。

### 4. 依赖现有值提交的 prompt，回归测试要锁“预填行为”而不是只锁标题

rename prompt 的 Enter 提交逻辑只会在输入非空时触发。若已有线程名时不做预填，测试会表现成“没有发 `SetThreadName` 事件”，但根因其实是 prompt 初始化状态退化了。

这类测试应该优先覆盖：

- 弹窗是否带入已有值
- 直接 Enter 是否能走提交路径

## 最小验证闭环

- `env -u RUSTC_WRAPPER cargo nextest run -p codex-tui`
- `env -u RUSTC_WRAPPER just fix -p codex-tui`
- `env -u RUSTC_WRAPPER cargo check -p codex-tui --tests`

其中最后一次 `check` 是因为仓库规则要求 `fix` 后不再重跑测试，所以收尾阶段至少要保一条编译面证据。

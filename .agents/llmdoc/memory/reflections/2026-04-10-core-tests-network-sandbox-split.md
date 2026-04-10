# core 测试在网络禁用环境下的拆分回路

## 背景
- 在 sandbox 环境里跑 `codex-core` 测试时，经常需要设置 `CODEX_SANDBOX_NETWORK_DISABLED=1` 来跳过需要真实网络的测试。
- 同时存在一条测试 `user_shell_command_does_not_set_network_sandbox_env_var` 必须验证该环境变量未设置。

## 发生了什么
- 直接在全量 `nextest` 里设置 `CODEX_SANDBOX_NETWORK_DISABLED=1` 会让上述测试失败。
- 不设置环境变量则会触发其它网络相关测试卡住或超时。

## 根因
- `CODEX_SANDBOX_NETWORK_DISABLED` 既是运行时沙箱信号，又会被特定测试显式断言为 “未设置”。
- 同一个测试回路里无法同时满足“全局禁网”与“断言未设置该变量”。

## 下次怎么做
- 将 `codex-core` 测试拆成两段执行：
  - 先跑禁网主回路（排除该测试）：
    `env -u CARGO_INCREMENTAL RUSTC_WRAPPER= CODEX_SANDBOX_NETWORK_DISABLED=1 cargo nextest run -p codex-core -E 'not test(user_shell_command_does_not_set_network_sandbox_env_var)'`
  - 再单独跑要求未设置环境变量的测试：
    `env -u CODEX_SANDBOX_NETWORK_DISABLED -u CARGO_INCREMENTAL RUSTC_WRAPPER= cargo nextest run -p codex-core -E 'test(user_shell_command_does_not_set_network_sandbox_env_var)'`

## 适用范围
- 在 sandbox 环境里需要跳过网络测试，同时又必须覆盖 `user_shell_command_does_not_set_network_sandbox_env_var` 的 `codex-core` 回路。

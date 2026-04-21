# 2026-04-21 sub-agent config 重载必须以启用中的 project layer 为门，并保住 live runtime 状态

## 背景

这次收尾 `ztok compression behavior switch` 后续漂移时，`codex-core` 里真正难的不是把 `build_agent_shared_config()` 改成“支持 `turn.cwd` 重载”，而是避免把 live turn 的运行态配置洗掉。

一开始如果简单用 `turn.config.cwd != turn.cwd` 作为开关，就会把本该只做 runtime cwd 覆盖的场景也重建成磁盘配置，直接丢掉只存在于内存里的状态，比如：

- session/test 注入的 `[zmemory]` path / namespace
- runtime provider 细节（例如运行时 base_url）
- 其它已经解析好的 turn 级 provider / sandbox / shell 状态

## 这次确认的做法

- `build_agent_shared_config()` 默认应以 live `turn.config` 为基线。
- 当 `turn.cwd` 与 live config 的 cwd 不同时，可以按 `turn.cwd` 重载一份 config，但不能直接替换。
- 判断是否切到重载结果时，至少要同时满足两点：
  - 新的 `ConfigLayerStack` 里确实存在 **启用中的** `Project` layer，而不是只有 disabled project layer；
  - 把 `config_layer_stack`、`active_project` 和 turn runtime 字段归一化后，重载配置仍和 live config 有实质差异。
- runtime provider 不仅要回灌到 `config.model_provider`，还要同步写回 `config.model_providers[model_provider_id]`，否则子 agent 在后续 provider 解析和快照里仍会拿到旧 provider 信息。
- `spawn_agent` / `spawn_agent_v2` 的 `provider` 参数必须先于 model override 生效，这样 `provider + model` 组合才能用目标 provider 的模型目录和推理能力约束来解析。

## 为什么值得记

- `turn.cwd` 差异本身不是“应该重载 project config”的充分条件。它只能说明运行目录变了，不能说明应该丢掉 live config。
- project-local config 在 untrusted 场景下仍可能出现在 layer stack 里，但它是 disabled 的。把这种 layer 当成“重载成功”的信号，会把很多本地/测试注入状态误清掉。
- provider 相关断言很容易只修字段，不修 provider map；子线程快照和后续模型解析会继续读 map，所以必须两边一起更新。

## 下次复用

- 以后再修 `turn.cwd`、project-scoped config、子线程派生配置时，先分清三类状态：
  - 磁盘 project-scoped 状态
  - live session/runtime 状态
  - 仅用于诊断的 layer / active-project 元数据
- 如果某条用例只在 `cargo nextest` 下超时，而 `cargo test -- --exact --nocapture` 能过，先把它记为 runner/调度差异，再继续查真正的逻辑回归，不要把 nextest 超时直接当成功能坏了。

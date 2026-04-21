# 2026-04-21 `/models` unsupported 不能只做进程内熔断，必须持久化并切到 OpenAI fallback refresh

## 背景

这次继续处理自定义 provider `GET /models` 返回 `404` 的启动卡住问题时，先前的修复只做到了：

- 当前进程内把该 provider 标记为“不再刷新 `/models`”
- 继续使用 bundled catalog

这能解决“同一进程里反复打失败接口”，但解决不了“每次新启动又先打一枪 `/models`”。

## 这次确认的事实

- `thread/start` 和 turn 里的 `list_models(OnlineIfUncached)` 会在每个新进程重新建 `ModelsManager`。
- 只靠内存里的 `remote_refresh_disabled`，跨进程重启后状态会丢失。
- 只退回 bundled catalog 虽然能保住可用性，但不会更新 cache，也不会阻止下个进程再次先打主 provider 的 `/models`。
- 用户真正需要的是：
  - 一旦确认当前 provider 不支持 `/models`，后续进程不要再先请求它。
  - 改为直接走 openai/codex provider 更新模型目录。
  - fallback 失败时允许失败，但不要再把 unsupported `/models` 当成每次启动都要探测一次的能力。

## 这次形成的实现原则

- 对 `404` / `405` / `501` 这类“接口不支持”的 `/models` 响应，除了进程内熔断，还要把 provider signature 持久化到 `codex_home` 下的单独状态文件。
- provider signature 只应包含非敏感字段，例如：
  - `base_url`
  - `wire_api`
  - `model_catalog`
- 新进程启动时，如果命中该持久化状态：
  - 不再先请求当前 provider 的 `/models`
  - `Online` / `OnlineIfUncached` 直接走 openai fallback provider 更新 cache
- fallback provider 不能硬编码，必须复用配置里已经解析好的 `openai` provider，这样 `openai_base_url` 覆盖仍然有效。
- `500`、网络错误、超时仍然是故障，不应写入 unsupported 持久化状态。

## 验证重点

- 两个不同 `ModelsManager` 实例共享同一个 `codex_home` 时：
  - 第一个实例主 provider `/models` 404 后会写入持久化状态
  - 第二个实例不会再请求主 provider `/models`
  - 第二个实例会直接请求 fallback provider `/models`
- 只编译通过 `models-manager` 不够；因为 fallback provider 需要从 `core` 的 `thread_manager` 和 `multi_agents_common` 传入，所以还要至少验证 `codex-core` 编译面。

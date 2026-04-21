# 2026-04-21 spawn_agent provider 默认模型与 profile 重载边界

- `spawn_agent` 增加 `provider` override 后，不能只切 `model_provider_id` 而继续继承父 turn 的 `model`。否则子 agent 会带着父线程的模型 slug 跨 provider 启动，既违背配置语义，也可能落到新 provider 不支持的模型上。修复时要把“provider 变更但未显式传 model”视为一次完整的模型决议切换：优先使用 provider 固定的 `model`，否则重新按该 provider 求默认模型，而不是保留父模型。
- 当 turn 因 `cwd` override 触发 config reload 时，显式选中的 `config_profile` / `active_profile` 也属于需要保留的 live runtime 选择，不能只把 `cwd` 传进 `ConfigOverrides`。否则 project layer 一旦生效，子 agent 会静默丢掉父线程已经选中的 profile，并连带丢掉 profile 派生配置。
- 这类修复的回归测试不能只看“provider id 变了”或“project config 生效了”。要专门构造一个非常态父模型，断言 provider override 后不会继续带着父模型；也要构造 profile-only 派生字段，断言 cwd reload 后 profile 仍然保留。

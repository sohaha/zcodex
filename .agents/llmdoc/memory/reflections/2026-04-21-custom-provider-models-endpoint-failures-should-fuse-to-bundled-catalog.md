# 2026-04-21 自定义 provider 缺少 `/models` 时，应熔断远端刷新并退回 bundled catalog

## 背景

这次排查 `codex` 启动后像“卡死”的问题时，日志表面上同时出现了 `announcement_tip`、plugins featured cache、shell snapshot、PostHog 和 app-server 启动等多条异步链路，但真正与启动体验相关的是 models refresh：

- thread/start 会同步跑 `list_models(OnlineIfUncached)`
- 当前会话 provider 已切到用户配置的 `model_provider_id`
- 自定义 provider `base_url = http://127.0.0.1:18100` 只支持 Responses，不支持 `GET /models`
- 本地 models cache 一旦 miss / stale / version mismatch，就会真的去打 `/models`

结果是：自定义 provider 明明已经能跑主业务，但只因为不支持模型目录接口，就在启动阶段反复触发远端探测。

## 这次确认的处理原则

- `model_catalog` 仍然是最高优先级的显式目录来源；有它时保持 `CatalogMode::Custom`，完全跳过 `/models` 刷新。
- 没有 `model_catalog` 时，`ModelsManager` 初始化出来的 bundled catalog 本来就足够作为启动兜底，不需要再偷偷回退去请求 OpenAI 远端目录。
- `/models` 返回 `404` / `405` / `501` 这类“接口不存在/不支持”的结果时，应把该 provider 在**本进程内**标记为“不再刷新 models”，并继续使用 bundled catalog。
- `500`、网络错误、超时等仍然属于真实故障，不能和“不支持接口”混为一谈；这些错误应继续保留失败语义和重试机会。

## 为什么不能默认回退到 OpenAI 远端目录

- 用户既然显式选了自定义 provider，就应该以该 provider 的能力和模型集为准。
- 静默切到 OpenAI `/models` 会把“provider 不支持目录接口”这个配置/兼容问题掩盖掉。
- 远端拉回来的模型元数据未必适用于当前 provider，尤其是上下文窗口、推理能力、额外 speed tier 这类字段。

## 可复用检查点

以后只要看到“自定义 provider 已能对话，但启动/模型选择卡住”，优先核对：

1. `ThreadManager` 是否已经从 hardcoded OpenAI provider 切到 `config.model_provider_id`
2. 当前 provider 是否真的实现了 `GET /models`
3. 本地 `models_cache.json` 是否因为 TTL、版本或格式原因失效
4. 失败后的行为是继续使用 bundled catalog，还是反复在线刷新

## 验证建议

- 至少补两类测试：
  - `/models` 404 后第一次失败应被收敛，第二次刷新不再发请求
  - `/models` 500 仍应继续报错，不能被错误地“熔断成功”

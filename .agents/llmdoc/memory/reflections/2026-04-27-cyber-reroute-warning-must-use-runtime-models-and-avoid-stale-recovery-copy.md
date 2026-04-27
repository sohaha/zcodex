# cyber reroute warning 必须使用运行时模型名，并避免陈旧恢复文案

## 背景

- 安全降级链路在 `codex-rs/core/src/session/mod.rs` 里检测到 `requested_model` 与 `server_model` 不一致时，会发一条用户可见 `Warning`。
- 实际请求已经被路由到 `glm/glm-4.7`，但 warning 仍然显示“已路由至 gpt-5.2”以及固定的“恢复访问 gpt-5.3-codex”文案。

## 发现

1. 告警文案把 fallback 目标模型和原请求模型都写死成固定字符串，没有使用运行时的 `requested_model` / `server_model`。
2. 这类安全告警一旦和真实路由结果脱节，用户会同时看到正确的 reroute 结果和错误的 warning，直接削弱可观察性。
3. 若产品当前不想展示 trusted access / learn more 这类恢复入口，应直接移除整段恢复文案，而不是保留可能继续过期的静态句子。

## 结论

- 安全降级 warning 的模型名必须始终来自运行时值：原模型取 `requested_model`，目标模型取 `server_model`。
- 用户可见告警只保留当前仍然真实、必要的信息；额外的恢复入口文案若不是稳定产品需求，应删掉而不是写死。

## 后续规则

- 以后修改 reroute / fallback / safety warning 时，先核对文案里的模型名、provider 名和链接是否都来自当前运行时或稳定配置源。
- 若用户界面已经有独立的 reroute 展示面，warning 仍应保持事实一致，但不要复制额外且容易过期的策略说明。

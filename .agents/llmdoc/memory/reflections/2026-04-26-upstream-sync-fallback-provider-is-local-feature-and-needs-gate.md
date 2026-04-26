# upstream sync 中 fallback provider 属于本地分叉功能

## 背景
- 本轮同步上游后，`fallback_provider` / `fallback_model` / `fallback_providers` 一度被当成旧残留处理，导致 request fallback provider chain 实现和测试缺失。
- 追溯本地提交后确认它不是 upstream 原生功能，而是本地分叉长期保留能力：
  - `2ddd4b5bd feat(core): add request fallback provider support`
  - `a00991eec feat(core): support fallback provider chains`
  - `7f982694a fix(core): preserve fallback request model and provider cache`
  - `3bef9a412 feat: restore fallback provider helper functions`

## 结论
- `fallback_provider`、`fallback_model`、`fallback_providers` 是本地功能，默认不能因 upstream 没有对应实现而删除。
- 它与 WebSocket transport fallback 不是同一个功能：前者在 request/provider 层切 provider/model，后者在同 provider 内从 WebSocket 切 HTTP transport。同步时两者都要保留。
- 恢复时不能只让编译通过，还要把本地行为写入 `.codex/skills/sync-openai-codex-pr/references/local-fork-features.json`，让 `local_fork_feature_audit` 成为后续 merge-back gate。

## 后续规则
- 遇到上游删除或测试模块看似过时时，先查本地提交历史与 `local-fork-features.json`，不要直接删测试。
- 对本地 request fallback provider 的验证至少覆盖：
  - 单一 `fallback_provider` + `fallback_model`
  - `fallback_providers` 链式重试
  - 未指定 fallback model 时保留 primary requested model 或 provider 默认 model
  - `UsageLimitReached` 也能进入 fallback provider 链
  - fallback warning 只走用户可见事件，不进入模型可见 history item
- 如果未来 upstream 提供等价 provider/model fallback，只有在配置 key、失败语义、事件可见性和测试覆盖都不回退时，才可以把本地实现迁移到 upstream 实现。

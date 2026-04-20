# upstream sync 的中文化检查还要覆盖 session 文案桥接

## 背景

这次在 `codex-rs/core/src/session/mod.rs` 补中文文案后，暴露出一个同步基线缺口：

- 文案源头在 `core/src/session/mod.rs`
- app-server 会把同一类 `SteerInputError` 重新映射成对外错误
- TUI 里又有依赖字符串前缀的解析逻辑
- 相关测试分散在 `analytics`、`core`、`app-server`

如果 `sync-openai-codex-pr` 只检查“中文字符串仍在源码里”，上游同步时很容易出现只改一层的半回归：

- `core` 改回英文，但 `tui` 仍按中文前缀解析
- `app-server` 映射回英文，而 `analytics` / `core` 测试仍锁中文
- warning 源头和 warning 前缀测试脱节

## 结论

### 1. `localized_behavior` 不能只看文案源头

对跨层透传的用户文案，基线检查至少要同时覆盖：

- 源头：真正生成文案的模块
- 桥接：下游重新映射或转发该文案的层
- 解析：依赖字符串前缀/全文匹配的消费层
- 测试：锁定该链路的直接断言

### 2. `turn/steer` 和 warning 前缀要独立成专项检查

这类字符串不是普通 UI 文案，它们带有运行时契约属性。

尤其是：

- active-turn race 的 missing / mismatch 文案
- `ActiveTurnNotSteerable` 的 review/compact 文案
- `警告：` 这种被测试和消息提取逻辑依赖的前缀
- `js_repl` 启动 warning 这类跨事件与测试断言的文案

这类链路应该单独建 feature，而不是只塞进泛化的中文哨兵集合里。

## 这次落地

- 为 `sync-openai-codex-pr` 新增 `session-warning-steer-localization-bridge`
- 在 skill 正文和 checklist 里补充规则：
  - 触及 `core/src/session/mod.rs`、`app-server/src/codex_message_processor.rs`、`tui/src/app.rs` 或同类 warning/steer 文案映射时，必须联动检查 `core -> app-server -> tui`
  - 不能只靠静态 grep；至少要保留 warning 前缀和 steer 文案两类回归测试锚点

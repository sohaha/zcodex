# 交接记录

## 当前焦点

- 更新时间：2026-04-04T12:33:21.807Z
- 本轮摘要：继续恢复被今天合并覆盖的功能：补了 codex-rs/core/src/codex.rs 的 per-turn user_instructions/AGENTS 重新解析与基线重注入条件，恢复了 turn cwd 切换后的 project-doc 生效；补了 codex-rs/tui/src/chatwidget.rs 的 /buddy 帮助与用法汉化；保留了 codex-rs/tui/src/buddy/render.rs 的 traits/status 第二行恢复。已定向验证 codex-core 的 user_instructions 重注入单测与 codex-tui 的 buddy status 单测通过；未做整仓测试，仍有大量 .snap.new 待后续按功能恢复情况继续清理。

## 待确认问题

- 暂无，若后续发现疑点请及时补充。

## 下一步检查

- 优先检查当前 diff、相关测试和受影响模块。

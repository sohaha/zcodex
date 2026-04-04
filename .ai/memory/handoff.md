# 交接记录

## 当前焦点

- 更新时间：2026-04-04T13:20:23.895Z
- 本轮摘要：恢复 TUI 汉化与显示细节：审批 overlay 标题/字段与待审批提示中文化，buddy 身份行补回可见/隐藏，请求用户输入不可用提示改中文；更新对应测试断言。已运行 just fmt；cargo nextest run -p codex-tui 失败（tui/src/clipboard_paste.rs 缺失 is_probably_wsl 等函数，非本次修改引入）；cargo nextest run -p codex-tools 通过。

## 待确认问题

- 2026-04-04 已确认：上游同步/合并可能覆盖分叉版自有功能与汉化；下次使用同步技能前，先做“分叉保留清单”审计，再做定向回归，避免再次把本地增强误当成可覆盖内容。

## 下一步检查

- 优先检查当前 diff、相关测试和受影响模块。
- 涉及上游同步时，优先核对 `core` 上下文链路与 `tui` 中文 UI/插件/buddy/approval 这些高风险区。

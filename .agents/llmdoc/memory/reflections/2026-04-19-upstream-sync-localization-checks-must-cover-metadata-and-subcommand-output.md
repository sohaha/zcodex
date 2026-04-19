# upstream sync 的中文化检查必须覆盖元数据源头与子命令输出

## 背景

这次继续收中文化回归时，暴露出两个不同层面的漏口：

1. TUI 首屏/引导文案虽然有中文化基线，但 `trust_directory.rs`、`history_cell.rs` 首屏帮助和
   `Feature::ExternalMigration` 这种元数据源头没有纳入 `sync-openai-codex-pr` 的本地特性检查。
2. CLI 主入口 `main.rs` 已有不少中文化哨兵，但像 `marketplace_cmd.rs` 这种真实用户会直接执行的
   子命令输出，仍可能整组留在英文，而且不会被当前同步基线发现。

## 结论

### 1. `localized_behavior` 不能只盯视图入口

如果用户可见文案来自下面这些位置，基线检查都应该直指真正源头：

- `FeatureSpec` / feature metadata
- onboarding 组件
- history/render 组件
- shared helper
- 直接字符串断言
- snapshot

只保视图入口文件，会漏掉“菜单项实际来自元数据”“文本被 snapshot 钉住”“测试断言仍锁英文”的情况。

### 2. CLI 中文化不能只保 `main.rs`

`main.rs` 的 help/localization 哨兵很重要，但它只覆盖顶层入口。

像 `marketplace_cmd.rs`、`mcp_cmd.rs` 这类子命令模块本身也会直接向用户输出文案。只检查
`main.rs`，无法发现这些子命令在 upstream sync 或本地重构后整段回退成英文。

### 3. 更稳的同步基线写法

对中文化特性，至少要混合三类哨兵：

- 顶层 CLI/help 哨兵
- 高频 TUI/onboarding/history 文案哨兵
- 子命令输出或 feature 元数据哨兵

必要时再加上直接断言或 snapshot 所在路径，避免“源码和测试保护面错位”。

## 这次落地

- 扩充了 `sync-openai-codex-pr` 的 `chinese-localization-sentinels`
  - `codex-rs/tui/src/onboarding/trust_directory.rs`
  - `codex-rs/tui/src/history_cell.rs`
  - `codex-rs/features/src/lib.rs`
- 在技能正文和 checklist 里明确：
  - 中文化 surface 要追真正源头
  - 要把直接字符串断言和 snapshot 一起纳入审查

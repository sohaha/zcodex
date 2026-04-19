# TUI 汉化需要同步更新源头、断言与 snapshot

## 背景

这次 TUI 汉化回归表面上看是几个界面字符串又变回了英文或半英文：

- 启动时的 trust directory 提示
- 首屏 history 帮助文案
- `/experimental` 菜单里的 `External migration`

但真正的根因不是同一层：

- `trust_directory.rs` 和 `history_cell.rs` 是直接渲染层，文案可能在上游同步或冲突解法里被回滚
- `External migration` 则来自 `codex-rs/features/src/lib.rs` 的 feature 元数据，不在 TUI 视图里定义

## 关键观察

### 1. TUI 文案回归不能只修渲染层

实验功能菜单中的名称和描述来自 `FeatureSpec.stage = Stage::Experimental { ... }`。如果只检查
`experimental_features_view.rs`，会误以为视图层漏翻，结果补错位置。

处理这类菜单项时，先追到元数据源头，再确认视图只是消费该元数据。

### 2. 汉化回归要同时改三类事实源

只改源码字符串不够，至少还要同步：

- 直接字符串断言的测试
- 用户可见 TUI 的 snapshot

否则后续要么测试失败，要么 snapshot 继续把旧英文钉住，下一次冲突解法又容易把中文覆盖回去。

### 3. dirty worktree 下要先确认 staged 边界，再做区块级编辑

`history_cell.rs` 这类高频文件在脏工作区里很常见。继续修复前先看 `git diff --cached` 和任务相关
文件集，确认没有真实区块冲突，再只改本任务相关块，避免把并行任务内容误卷进来。

## 验证边界

这次更稳妥的最小验证闭环是：

- `just fmt`
- `cargo check -p codex-tui --lib`

如果 `cargo test -p codex-tui` 或 `cargo test -p codex-features` 被仓库现有无关 test compile 问题
阻塞，要明确把这些阻塞与本次汉化改动分开记录，而不是把测试失败直接归因到本次修改。

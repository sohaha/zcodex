# TUI 中文化与格式一致性改进反思

## 背景

这次任务包含一批 TUI 相关的小型改进：

1. **中文化输出细化**：`ascii_animation.rs`、`request_user_input/mod.rs`、`history_cell.rs`
2. **格式统一**：`markdown_render_tests.rs` 中 `Line::from_iter` 风格一致化
3. **清理备份文件**：删除 `models-manager/src/manager.rs.{backup,backup2,bak}`

## 关键观察

### 1. 中文化输出的持续完善

TUI 中文化不是一次性工作，而是在使用过程中不断发现细节：

- **ascii_animation**：中文字符宽度、对齐与英文不同，需要调整动画帧渲染逻辑
- **request_user_input**：提示文本、按钮标签需要保持中文一致性
- **history_cell**：历史记录中的状态文本需要本地化

### 2. Line::from_iter 的两种写法

`markdown_render_tests.rs` 中暴露了两种等效但风格不同的写法：

```rust
// 单行压缩
let expected = Text::from(Line::from_iter(["path".cyan()]));

// 多行展开
let expected = Text::from(Line::from_iter([
    "path".cyan(),
]));
```

判断原则：

- **优先单行**：当数组内容可以安全放在一行内，使用单行压缩
- **必要时展开**：当代码因单行过长而被 rustfmt 折行时，直接使用多行展开形式
- **避免来回 churn**：不要在没有可读性收益的情况下在两种形式间转换

### 3. 备份文件清理

`.backup`、`.backup2`、`.bak` 文件不应该提交到版本控制：

- 应该通过 `.gitignore` 或构建脚本自动排除
- 手动清理时使用 `git rm` 并确认变更范围

## 经验教训

1. **中文化是渐进过程**：不指望一次完成所有 TUI 文本的本地化，而是在实际使用和测试中持续发现盲点

2. **格式一致性需要权衡**：
   - rustfmt 会处理大部分格式问题
   - 但 Line::from_iter 的单行/多行选择需要人工判断
   - 统一风格时避免过度重构，优先保证代码可读性

3. **测试快照要跟上中文化**：
   - 每次中文化变更都需要同步更新 insta snapshot
   - 参考已有经验 `2026-04-14-upstream-sync-tui-localization-snapshot-loop.md`

## 后续行动

- 考虑在 TUI 模块建立中文文案清单，避免遗漏
- 对于常见模式（如 Line::from_iter），可以在本地编码约定中记录推荐风格
- 备份文件清理应该自动化，不应该依赖手动 `git rm`

## 相关文档

- `.agents/llmdoc/memory/reflections/2026-04-14-upstream-sync-tui-localization-snapshot-loop.md`
- `.agents/llmdoc/architecture/runtime-surfaces.md`（TUI 职责边界）
- `codex-rs/tui` file-local conventions

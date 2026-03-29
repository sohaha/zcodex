# 交接记录

## 当前焦点

- 更新时间：2026-03-29T05:00:00Z
- 本轮摘要：已提交 `zmemory` 对齐实现（`5b7923bd3`），并继续处理多会话下的 Cargo build lock 效率问题。新增 `codex-rs/scripts/resolve-cargo-target-dir.sh`，把 `just` 里的 `fix` / `clippy` / `test` / `core-test-fast` / `app-server-test-fast` / `native-tldr-test-fast` 默认分配到独立 target 子目录；如需把同一条命令再拆到独立 lane，可设置 `CODEX_CARGO_LANE=<name>`。若必须完全沿用外部传入的 `CARGO_TARGET_DIR`，可设置 `CODEX_CARGO_TARGET_EXACT=1`。

## 待确认问题

- `just fix` 在超大工作区上仍可能耗时很长，但不再默认与 nextest/其他 cargo 流程争用同一个 target 目录锁。
- 当前仓库仍有大量无关脏改动，后续提交必须继续精确挑选文件。

## 下一步检查

- 优先验证新 target-dir 分流在实际并行 cargo/test/fix 流程中的表现。
- 如继续做 `zmemory` 或 `native-tldr`，优先使用新的 `just` lane 方案，必要时显式设置 `CODEX_CARGO_LANE`。

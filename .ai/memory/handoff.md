# 交接记录

## 当前焦点

- 更新时间：2026-03-29T05:10:00Z
- 本轮摘要：继续收敛 Cargo build lock。除先前已分流的 `fix` / `clippy` / `test` / `core-test-fast` / `app-server-test-fast` / `native-tldr-test-fast` 外，现又把 `just` 里的高频 `cargo run/build` 入口也改为独立 target 子目录：`codex`、`exec`、`file-search`、`app-server-test-client`、`mcp-server-run`、`write-config-schema`、`write-app-server-schema`、`write-hooks-schema`、`log`。`app-server-test-client` 同步改为读取分流后的 `$CARGO_TARGET_DIR/debug/codex`。同一命令需要再并行时，继续用 `CODEX_CARGO_LANE=<name>`；若必须完全沿用外部 `CARGO_TARGET_DIR`，可设置 `CODEX_CARGO_TARGET_EXACT=1`。

## 待确认问题

- `cargo fetch` / 包缓存层面的锁未单独处理；当前主要解决的是 build target 锁冲突。
- 当前仓库仍有大量无关脏改动，后续提交必须继续精确挑选文件。

## 下一步检查

- 优先观察多会话并行执行 `just codex`、`just fix`、`just core-test-fast` 时是否还出现 target build lock 等待。
- 如仍有瓶颈，再评估是否需要对 `cargo fetch` / registry cache 层做额外隔离或串行门控。

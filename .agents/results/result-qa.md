# QA Review

- Status: FAIL
- Summary: 审查 `HEAD~1..HEAD` 后确认一处高严重度回归：`InProcessAppServerClient` 现在会在客户端层直接拒绝 `ChatgptAuthTokensRefresh` server request，但嵌入式 TUI 正是通过这条 in-process 路径运行，并且现有 TUI 逻辑明确把该请求视为可处理请求而非 unsupported。结果是 ChatGPT token 过期后的刷新流程在 embedded TUI 中会被短路，无法到达 UI 处理层。
- Files changed:
  - `.agents/issues/2026-04-28-context-hooks-architecture.toml`
  - `.agents/llmdoc/index.md`
  - `.agents/llmdoc/memory/reflections/2026-04-28-auth-401-should-fallback-without-retrying-the-same-provider.md`
  - `codex-rs/app-server-client/src/lib.rs`
  - `codex-rs/app-server/src/codex_message_processor.rs`
  - `codex-rs/app-server/src/in_process.rs`
  - `codex-rs/cli/tests/ztok.rs`
  - `codex-rs/core/src/codex_thread.rs`
  - `codex-rs/core/src/config/config_tests.rs`
  - `codex-rs/core/src/memories/prompts_tests.rs`
  - `codex-rs/core/src/session/mod.rs`
  - `codex-rs/core/src/session/turn.rs`
  - `codex-rs/core/src/tools/handlers/tldr.rs`
  - `codex-rs/core/templates/compact/ztok.md`
  - `codex-rs/core/tests/suite/websocket_fallback.rs`
  - `codex-rs/mcp-server/src/tldr_tool.rs`
  - `codex-rs/native-tldr/src/daemon.rs`
  - `codex-rs/native-tldr/src/semantic.rs`
  - `codex-rs/native-tldr/src/semantic/embedder.rs`
  - `codex-rs/ztok/src/lib.rs`
  - `codex-rs/ztok/src/session_cache_cmd.rs`
  - `codex-rs/ztok/src/settings.rs`
- Acceptance criteria checklist:
  - [x] 已审查 `HEAD~1..HEAD` 的变更文件与关键调用点。
  - [x] 已给出带 `file:line` 的已验证 findings。
  - [x] 未修改源码。
  - [x] 已记录自动化验证的实际覆盖与受限项。

## Review Result: FAIL

### CRITICAL
- None.

### HIGH
- `codex-rs/app-server-client/src/lib.rs:298` — `forward_ready_in_process_event()` 会把 `ServerRequest::ChatgptAuthTokensRefresh` 直接失败返回，而不是转发给 in-process 客户端。这个假设和现有产品面冲突：embedded TUI 通过 `InProcessAppServerClient::start` 启动 app-server（`codex-rs/tui/src/lib.rs:271`），并且其请求管理器明确把 `ChatgptAuthTokensRefresh` 视为正常可处理请求（`codex-rs/tui/src/app/app_server_requests.rs:690`）。当前改动会让 token 过期后的刷新流程在 embedded TUI 中被客户端层短路，用户无法完成 auth 恢复。 — remediation code:
```rust
// codex-rs/app-server-client/src/lib.rs
// 不要在共享 in-process client 层拒绝 ChatgptAuthTokensRefresh；
// 让它和其它 ServerRequest 一样继续转发给上层消费。
forward_in_process_event(event_tx, skipped_events, event, |request| {
    let _ = request_sender.fail_server_request(
        request.id().clone(),
        JSONRPCErrorError {
            code: -32001,
            message: "in-process app-server event queue is full".to_string(),
            data: None,
        },
    );
})
.await
```

### MEDIUM
- None.

### LOW
- None.

## Verification Notes

- 代码审读证据：
  - `codex-rs/app-server-client/src/lib.rs:298-316` 新增了对 `ChatgptAuthTokensRefresh` 的共享拒绝分支。
  - `codex-rs/tui/src/lib.rs:271-289` 证明 embedded TUI 生产路径使用 `InProcessAppServerClient::start`。
  - `codex-rs/tui/src/app/app_server_requests.rs:690-703` 证明 TUI 现有逻辑预期该请求会到达 UI 层处理，而不是被底层判为 unsupported。
- 自动化命令：
  - `codex ztok shell bash -lc 'env -u CARGO_INCREMENTAL -u RUSTC_WRAPPER cargo test -p codex-app-server-client in_process_thread_start_buffers_startup_warning_before_response -- --exact'`
    - 结果：20 分钟超时，未形成可用结论。
  - `codex ztok shell bash -lc 'env -u CARGO_INCREMENTAL -u RUSTC_WRAPPER cargo test -p codex-native-tldr query_daemon_read_timeout_reports_unresponsive_daemon --lib -- --exact'`
    - 结果：crate 完成编译，但 `--exact` 过滤未命中测试名，0 tests run。
  - `codex ztok shell bash -lc 'env -u CARGO_INCREMENTAL -u RUSTC_WRAPPER cargo test -p codex-native-tldr semantic_index_batches_document_embedding_generation --lib -- --exact'`
    - 结果：crate 完成编译，但 `--exact` 过滤未命中测试名，0 tests run。
- 工具受限：
  - `ztldr change-impact / warm / status` 均返回 `structuredFailure: tool_error`，原因为 native-tldr socket `read timeout`，因此本次结构影响分析退回到 `git diff + 调用点审读`。

## Residual Risk

- 除上述高严重度回归外，其余改动未发现已验证的中等级以上问题，但 `app-server-client` / `core` 相关定向测试本次未能完整跑通，仍存在未被自动化覆盖的残余风险。

## 背景
- a2 已经把单机 federation daemon、本地 IPC 和 JSON state 清理路径落在 `codex-federation-daemon`。
- a3 需要继续提供 `codex federation ...` 命令面，让用户能启动 daemon、注册实例、列 peers、发送文本任务、读 inbox 和写 ack。

## 观察
- 如果直接在 `cli/src/federation_cmd.rs` 里自己写 socket 连接、endpoint 读取和 JSONL request/response，会很快和 daemon 的隐藏 internal 入口、后续脚本调用、以及未来的 app/bridge 接线混成一团。
- `native-tldr` 的 lifecycle 经验说明：CLI 负责显式命令面和进程拉起，真正的本地连接协议应该压到单独 client 层，否则后续每个命令都会复制一份“读 endpoint -> 连接 -> 发 JSON -> 读一行 response”的样板。

## 结论
- federation 的层次应该固定成：
  - `federation-protocol`：实例卡片、信封、ack、cleanup 报告和 daemon command/response 契约
  - `federation-daemon`：本地 endpoint、状态持久化和过期清理
  - `federation-client`：薄连接层，统一处理 endpoint 路径和 JSON 命令往返
  - `cli/src/federation_cmd.rs`：只做命令解析、daemon 自启动和用户输出
- CLI 可以保留 hidden internal daemon 子命令来承载前台 daemon 进程，但 visible `codex federation ...` 命令不应自己内联 socket 协议。

## 本次处理
- 新增 `codex-rs/federation-client/`，把 `Ping` 和通用 `send` 都收进独立 client crate。
- 新增 `cli/src/federation_cmd.rs`，提供 `daemon start|ping|stop`、`register`、`peers`、`send`、`inbox`、`ack`，并通过 hidden `internal-daemon` 启动前台 daemon。
- `register/send/inbox/ack/peers` 全部先走 `FederationClient`，再和 daemon 交互，没有复用现有 `spawn_agent` / `InterAgentCommunication` 入口。

## 可复用经验
- 只要一个 CLI 子系统既有“启动本地后台进程”又有“对它发命令”的需求，优先拆 `daemon` 和 `client` 两层，不要把连接协议散进多个命令 handler。
- hidden internal 子命令适合承载真正的长期运行进程；visible `start` 命令负责 spawn + readiness probe，后续命令再通过 client 发请求。
- 当 federation 继续演进时，新增命令优先往 `federation-protocol` 和 `federation-client` 补契约，不要直接在 `cli` 里堆更多 socket/serde 细节。

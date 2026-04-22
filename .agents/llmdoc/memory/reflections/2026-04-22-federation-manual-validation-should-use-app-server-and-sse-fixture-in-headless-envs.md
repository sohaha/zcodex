# federation 手工验收在无终端环境下应优先走 app-server + SSE fixture

## 背景

在为主 `codex` 的 federation bridge 做最后的手工端到端验收时，目标是验证两份独立工作目录中的真实实例可以完成：

- 注册到同一个 federation daemon
- 列出 peers
- 向对方发送 `TextTask`
- 收到 `TextResult`

最开始直觉上会尝试直接启动两个交互式 `codex` TUI 实例，再用 `codex federation ...` 命令面做观测。

## 这次踩到的坑

- `codex exec` 没有“空闲驻留等待 federation 任务”的路径；无 prompt 且空 stdin 会直接报错。
- 在当前这类无真实终端仿真器的环境里，即使用 `script` 包一层伪 PTY，TUI 仍会卡在终端能力探测/查询阶段，导致还没走到 `thread/start` 注册 bridge，peers 为空。
- `codex app-server` 虽然适合 headless 启动，但默认模型请求可能走 Responses WebSocket；如果手工搭的 mock 只支持 `POST /v1/responses` SSE，会在真正执行 federation turn 时收到 `ws://.../v1/responses` 的 404。

## 这次确认的可靠做法

在无终端环境下，主 `codex` federation 的手工验收应优先改走：

1. 启动两个 `codex app-server --listen ws://127.0.0.1:PORT` 实例
2. 通过 websocket JSON-RPC 发送 `initialize` / `initialized` / `thread/start`
3. 在 `thread/start` 参数里显式传 `federation`
4. 给 app-server 进程设置 `CODEX_RS_SSE_FIXTURE`

这样有两个关键收益：

- `CODEX_RS_SSE_FIXTURE` 会让 model client 直接从本地 SSE fixture 读流，同时关闭 Responses WebSocket 路径，避免手工 mock 再补一套 websocket Responses 服务。
- `app-server` 正好命中这次 federation bridge 的真实 seam，能在不依赖 TUI 终端行为的情况下验证注册、心跳、任务桥接和结果回投。

## 结果

按上面的方式，已经在两个不同 cwd 的真实 app-server 实例上完成端到端检查：

- `peers` 返回两个实例卡片与心跳
- `send` 返回 `accepted`
- 收件方 bridge 将 `TextTask` 变成普通本地 turn
- 发件方 inbox 收到 `text_result = "fixture hello"`

## 后续默认做法

以后再做 federation bridge 的手工验收时：

- 有真实终端仿真器时，可以测交互式 `codex`
- 在 headless / CI / 容器环境里，优先用 `codex app-server` + websocket JSON-RPC + `CODEX_RS_SSE_FIXTURE`
- 不要先花时间给临时 mock 补 Responses WebSocket，除非任务本身就在验证 websocket transport

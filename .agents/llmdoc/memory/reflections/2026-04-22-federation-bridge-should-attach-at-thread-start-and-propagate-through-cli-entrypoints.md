# federation bridge 应挂在 thread/start seam，并贯通所有 CLI 入口

## 背景

在给主 `codex` 接入可选 federation 时，最容易走偏的方向是把跨实例消息硬塞进现有
`InterAgentCommunication`、`pending_input` 或新增 `SessionSource::Federation`。
这样虽然表面上“复用”了现有多子代理链路，但会把单机 federation 的实例注册、邮箱和 ack
语义污染进现有 root tree / multi-agent 内核，改动面会迅速扩大。

## 这次确认的正确 seam

- 最薄、最稳的桥接点是 `app-server` 的 `thread/start`。
- 新线程创建完成后立即注册 federation 实例并拉起后台 bridge task。
- bridge 只在本地线程空闲时轮询 inbox，把 `TextTask` 转成一次普通 `Op::UserTurn`。
- turn 完成后回投 `TextResult`，失败则写 `Rejected` ack。
- 不改 `InterAgentCommunication`、`pending_input` 语义，也不新增 `SessionSource::Federation`。

## 为什么这样更稳

- `tui` 和 `exec` 都已经统一经过 `thread/start` 建线程，桥接只要把 federation 参数透传到这里即可。
- app-server 层天然知道线程生命周期、cwd 和初始配置，适合注册实例卡片和后台心跳。
- 这样 federation 仍然是“外接能力”，daemon/协议/ack 都留在独立 crate 与独立 state root 中。

## 实施时容易漏掉的地方

- 不只要改 `thread/start` 协议，还要把 `tui`、`exec`、顶层 `codex resume/fork` wrapper 的 CLI 合并逻辑一起接通。
- `resume/fork` 如果不把 federation flags 从子命令 merge 回 `TuiCli`，参数会在顶层 wrapper 丢失。
- `thread/resume` 这版不应偷偷启用 federation bridge；当前桥接只对 fresh `thread/start` 生效，避免语义扩散。
- app-server 端到端验证要真的起 daemon，并通过 `thread/start` 发 federation 参数，再从 daemon inbox 看到 `TextResult`，不能只测参数 round-trip。

## 后续默认做法

以后再给主线程引入类似“外接运行时桥接”能力时，优先找 `app-server thread/start` 这种现成生命周期 seam，
先保证入口参数和后台任务的最小闭环，而不是先碰 `core` 里的多代理消息语义。

# 2026-04-12 Windows native-tldr daemon-first 收口反思

## 背景
- 用户追问“为什么 macOS 只需要 `codex-cli`，但 Windows 似乎需要单独 native-tldr”。
- 代码事实是 `codex-cli` 早已依赖 `codex-native-tldr`，Windows 缺的不是独立安装物，而是非 Unix daemon-first 的客户端查询、auto-start 和 stop/status 生命周期接线。
- `native-tldr` 里已有 TCP listener 雏形，但 `query_daemon()` 在非 Unix 固定返回 `None`，导致 Windows 长期只走本地引擎 fallback。

## 本轮有效做法
- 不把问题误判成“平台必须拆包”，而是直接收口到 IPC/lifecycle contract：Unix 继续走 Unix socket，非 Unix/Windows 改走 loopback TCP。
- 复用既有 artifact 布局，不额外引入新 metadata 文件名；非 Unix 直接把 `127.0.0.1:<port>` 写入现有 `.sock` 路径作为 endpoint metadata。
- 让 daemon health、query、stop、stale cleanup 共用同一套 metadata 语义，避免 TCP 监听、pid 文件、CLI 判断各说各话。
- 在 CLI 侧继续保持 hidden `internal-daemon` auto-start，不新增独立服务安装或系统常驻进程概念。

## 关键收益
- Windows 也回到与 macOS/Linux 一致的产品结论：只交付 `codex` / `codex-mcp-server`，不是额外发一个 native-tldr 安装包。
- 非 Unix/Windows 终于能复用 daemon cache/session，而不是每次都退回本地引擎冷启动。
- stop/status/health 有了可解释的非 Unix 行为：`Shutdown` 显式退出；endpoint 不可达时清理 stale metadata。

## 踩坑
- 看到 `.sock` 文件名不要机械地等同于 Unix socket；跨平台 artifact 名称可以稳定，但其承载内容未必相同。
- 执行阶段回写 issue 状态时不要假设有 `python3`；当前环境没有该命令，改用 `node` 才完成 Cadence `execution-write` 校验。
- `just fix` 在测试之后跑完，会继续改源码；后续应先审 diff，再收文档和 issue 状态，避免误报“已验证最新内容”。

## 后续建议
- 后续再补 Windows 专属条件编译/集成测试时，优先覆盖 endpoint metadata 损坏、端口不可达、Shutdown 超时这三类真实故障。
- 若未来要进一步拆小 `daemon.rs`，优先把 artifact/health 与 transport 分层，避免 Unix socket 与 TCP 分支继续混在同一大文件里。

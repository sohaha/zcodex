# ztok behavior mode switch 应由 CLI 桥接，并把 basic 做成完整的无会话缓存模式

## 背景

2026-04-21 为 `ztok` 新增 `[ztok] behavior = "enhanced" | "basic"` 配置时，真正容易做错的点不在枚举本身，而在行为边界：如果只在 `ztok` 命令内部局部关掉某一层压缩，最终会留下“命令输出还是被 session dedup / near-diff / sqlite 持久化介入”的混合态。

## 这次确认的做法

- 配置入口应留在 Codex 全局配置系统里，由 `codex-rs/cli` 在进入 `codex-rs/ztok` 前桥接成运行时环境变量；不要让 `codex-rs/ztok` 自己解析全局 `config.toml`。
- `basic` 必须是完整行为模式，而不是“部分关闭增强功能”：
  - `read` 仍保留本地过滤、窗口和行号能力；
  - `json` / `log` 退回原始文本；
  - `summary` 仍做本地启发式摘要，但 JSON / 日志输入退回通用文本摘要；
  - session dedup、near-diff 和 `.ztok-cache` SQLite 写入必须整体不可达。
- `summary` 的增强模式也要单独收口，不能只顾 `basic`：
  - dedup 身份至少要包含命令本身，而不是只看摘要文本或 `success`；
  - 会话缓存里只应持久化渲染后的摘要，而不是完整原始 stdout/stderr。

## 为什么值得记

- `ztok` 是共享命令面，`codex ztok` 和 alias `ztok` 一旦桥接位置做错，就会出现配置只影响其中一个入口的分叉。
- “兼容模式”最容易变成半关闭实现；如果不先把 session cache 边界锁死，测试只看终端输出时很容易漏掉后台写缓存和 near-diff 仍在工作的回归。
- `summary` 既运行命令又做压缩摘要，天然更容易把原始输出持久化边界做宽；以后再改其复用逻辑时，应优先检查 dedup signature 和 snapshot 内容，而不是只看最终屏幕文案。

## 下次复用

- 再给 `ztok` 增加运行模式时，先定义“哪些链路必须整体不可达”，再写 gate。
- 涉及 `ztok` 模式切换时，默认补两类测试：
  - CLI 级双模式回归，验证 `codex ztok` / alias `ztok` 一致；
  - `summary` 的身份与 snapshot 边界测试，防止不同命令误命中 exact dedup 或重新落完整原始输出。

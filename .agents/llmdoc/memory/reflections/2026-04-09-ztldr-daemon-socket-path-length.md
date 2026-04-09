# 2026-04-09 ztldr daemon socket 路径过长反思

## 背景
- 用户在 macOS 上运行 `codex ztldr daemon ...` 时，表面报错是 `daemon launch lock held; another process is starting it`。
- 继续前台运行 `codex ztldr internal-daemon --project ...` 后，真实根因暴露为 `bind socket ...` 与 `path must be shorter than SUN_LEN`。
- 触发条件是 daemon socket 默认落到 `std::env::temp_dir()`；在 macOS 上这通常是较长的 `/var/folders/...`，再拼接 `codex-native-tldr/<uid>/<hash>/codex-native-tldr-<hash>.sock` 后超过 Unix domain socket 的路径上限。

## 本轮有效做法
- 先把“launch lock 提示”与“前台 daemon 真实 bind 错误”区分开，避免继续围绕锁文件或 stale artifact 打转。
- 修复点放在 `native-tldr/src/daemon.rs` 的 daemon artifact 路径选择层，而不是在启动失败后继续扩充重试或保护分支。
- 保持路径选择是纯粹、确定性的：优先沿用绝对 `XDG_RUNTIME_DIR`，但如果该 scope 下推导出的 socket 路径会超出 Unix socket 长度上限，就统一回退到较短的 `/tmp/codex-native-tldr/<uid>`。
- 让 socket、pid、lock、launch lock 共享同一套 project-aware scope 选择，避免只迁 socket 造成运行时元数据分裂。

## 关键收益
- macOS 默认无需手工设置 `XDG_RUNTIME_DIR=/tmp/...` 也能启动 ztldr daemon。
- Linux 等已有短 `XDG_RUNTIME_DIR` 的场景保持原路径，不引入额外漂移。
- 非 Unix 平台继续沿用原来的非 socket 路径分支，不扩大改动面。

## 踩坑
- `daemon launch lock held` 只是启动未 ready 时的表层信号，不等于根因就是锁竞争；要前台跑 `internal-daemon` 才能看到 bind 失败。
- 不能简单把 daemon root 的 fallback 写成“目录不可写就改路径”；daemon IPC 路径需要确定性，路径选择应基于静态约束（这里是 socket 路径长度），而不是运行时写盘偶然性。
- 测试里原先只覆盖了 `XDG_RUNTIME_DIR` 是否绝对路径，没有覆盖“绝对但过长”的 Unix socket 场景，这会让 macOS 类问题漏过 CI。

## 后续建议
- 以后排查 ztldr daemon 启动失败，优先把 `codex ztldr daemon ...` 的结构化错误和 `codex ztldr internal-daemon --project ...` 的前台错误对照看。
- 所有新的 Unix socket 路径策略都应显式加一条“超长 runtime root 仍能回退到短路径”的单测，避免再次只在 macOS 上暴露。

# ztok 收口时要用干净 worktree 证明共享测试失败是否属于当前 issue

## 背景

2026-04-21 在收口 `ztok` 下一阶段路线图的 `a1` 时，`.agents/issues/2026-04-21-ztok-next-stage-compression-roadmap.toml` 一度把 issue 写成 `blocked`，理由是 `cargo test -p codex-core --lib` 失败。与此同时，`codex-cli --test ztok` 与 `codex-ztok` 定向验证已经通过，代码边界上也已完成 `runtime settings` 桥接和 `session_cache` 拆分。

## 这次确认的做法

- 当共享大套件失败而定向验证已经通过时，不要直接把 issue 留在 `blocked`；先把当前改动应用到干净 worktree 里复跑，再拿纯净 `HEAD` 做同命令对照。
- 对 `ztok` 这类局部 Rust 改动，隔离验证至少应拆成两组证据：
  - 改动 worktree：`just fmt`、`cargo test -p codex-cli --test ztok`、`cargo test -p codex-ztok`
  - 纯净基线 worktree：复跑共享失败项，例如 `cargo test -p codex-core --lib`
- 如果共享失败在改动 worktree 与纯净 `HEAD` 上完全一致，就应把它记为“当前分支基线失败”，而不是继续作为当前 issue 的阻塞理由。

## 为什么值得记

- 只看脏工作区或只看当前 worktree，很容易把并行改动、仓库漂移或既有失败误记成“实现半成品”。
- `Cadence` issue 的 `status` / `validate_status` 会直接影响后续执行判断；假阻塞会让已经闭环的 issue 继续挂着，干扰后续依赖链。
- 这次同时暴露了另一条边界经验：即使 issue 已闭环，staged diff 里仍可能混入无关 hunk，收口时要把“功能是否完成”和“提交边界是否干净”分开审查。

## 下次复用

- 当 `validate_by` 含共享 crate 大套件且失败看起来与当前改动无关时，默认执行：
  1. 在干净 worktree 应用当前 patch
  2. 在该 worktree 跑定向验证
  3. 在纯净 `HEAD` worktree 跑同一共享失败项
  4. 按对照结果回写 issue 状态
- 回写 notes 时要明确写出：
  - 哪些命令在改动 worktree 通过
  - 哪些共享命令在纯净 `HEAD` 也失败
  - 哪些属于提交边界问题但不影响当前 issue 完成定义

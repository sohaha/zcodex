# 2026-04-19 Embedded ZTOK 提示应改教逻辑 launcher + 专用 shell 入口

- 这次问题不是 `ztok` 运行时不会做事，而是 Embedded ZTOK developer instructions 本身把
  `err` / `test` 描述成“任意命令的泛化包装器”，导致 agent 面对 `git log` 这类需要标准输出的命令时，
  也会先走 `ztok err`，随后因为拿不到 stdout 再补一次命令。
- 同一份提示还直接向模型展示了 `"$CODEX_SELF_EXE" ztok ...`，这会把内部执行路径带进用户可见的 commentary /
  命令示例，和运行时“只显示逻辑 launcher 命令、绝对路径只留给内部执行”的边界相冲突。

## 本轮收口

- 在 `codex-rs/ztok/src/lib.rs` / `codex-rs/ztok/src/runner.rs` 增加 `ztok shell`：
  - `ztok shell <command> [args...]` 作为泛化 shell 命令入口，保留正常 stdout/stderr。
  - `ztok shell --filter err|test ...` 承接原先泛化包装器的“错误过滤 / 测试失败摘要”语义。
- 把 `core/templates/compact/ztok.md` 改成：
  - 明确 `CODEX_SELF_EXE` 只是内部 plumbing，不应出现在用户界面。
  - 强制代理时优先教 `codex ztok ...` 这种逻辑 launcher 形式。
  - 对任意 shell 命令优先教 `codex ztok shell ...`，不要再把 `err` 当通用入口。
- 用 `core/tests/suite/pending_input` 的三个 snapshot 用例验证 prompt 注入链路，而不是依赖
  已有漂移严重的 `codex-core --lib`。

## 后续规则

- 只要某个提示既是模型输入、又会间接反映到用户可见 commentary，就不要在里面教内部绝对路径或环境变量展开形式；
  优先教“逻辑命令”，把真实执行路径留给 runtime rewrite / launcher 解析层。
- 当 `ztok` 需要承接“任意命令”的模型心智时，应该给它一个语义自描述的专用入口（如 `shell`），不要再借用
  `err` / `test` 这类明显带语义偏向的 wrapper 名称。
- 验证这类改动时，至少同时覆盖：
  - `codex-cli --test ztok` 的子命令 surface / help / 直接行为
  - `core/tests/suite/pending_input` 对 developer instructions 的实际注入快照

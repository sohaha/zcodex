# 2026-04-19 Embedded ZTOK prompt 应保持紧凑，测试锁行为边界而不是整句文案

- `Embedded ZTOK` 这类常驻 developer prompt 很容易越修越长：为修一次错误心智补一句，为防回归再补一句，最后 prompt 里既有实现背景、又有重复规则、还有长示例，token 成本明显高于收益。
- 这次收口后，真正稳定且必要的内容只有四类：
  - 用户可见层只展示逻辑命令 `codex ztok ...`
  - 通用命令默认 `codex ztok shell <command> [args...]`
  - `--filter err` / `--filter test` 只是过滤模式，不是泛化入口
  - 不要把自动 rewrite 当成默认规划策略；复合 shell 语法时显式包真实 shell

## 测试经验

- `prompts_tests.rs` 不该再逐句锁 prompt 文案；否则每次压缩 prompt 都会引入高噪音失败。
- 对这类 prompt，单测应只锁最小语义锚点：
  - 标题存在
  - `codex ztok shell` 存在
  - `--filter err` / `--filter test` 存在
  - 旧错误心智文本不再出现
  - 路由结构锚点 `[shell_command routed via embedded ZTOK]` / `[shell_command kept raw]` 仍在
- `pending_input` snapshots 的价值在于验证“developer 指令仍被注入到首轮和 follow-up 请求”，不是逐句审 prompt；接受一次整段压缩即可，不要把它们当文案金标准。

## 后续规则

- 常驻 prompt 优先写短规则，不写实现背景。
- 只有会改变模型默认行为的句子才值得留在 prompt 里。
- 能放到 CLI help / 文档 / 运行时事件测试里的信息，不要重复塞进 developer prompt。

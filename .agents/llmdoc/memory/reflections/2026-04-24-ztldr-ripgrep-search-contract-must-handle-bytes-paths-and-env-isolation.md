# 2026-04-24 ztldr ripgrep 搜索契约必须同时处理 bytes payload、路径归一和环境隔离

## 背景
- `ztldr search` 主路径切到 `rg --json` 后，表面上解决了 walker + `read_to_string` 的整文件扫描问题，但底层契约出现了多处回归。
- 这轮排查确认的问题不是一个点，而是三类边界同时暴露：
  - `rg --json` 并不保证 `path.text` / `lines.text` 一定存在；非 UTF-8 文件名或行内容会落到 `bytes` 分支。
  - 以 `project_root` 为 cwd 搜索 `.` 时，`rg` 返回的路径会带 `./` 前缀，不能直接当作对外 `SearchMatch.path`。
  - `rg` 会继承有效的 `RIPGREP_CONFIG_PATH`，导致 `ztldr search` 结果被用户机器环境静默污染。

## 这轮有效做法
- 对 `rg --json` 统一走 `text/bytes` 双分支解析，`bytes` 用 base64 解码后再做 lossy UTF-8 还原，避免单个异常文件把整次 search 直接打成 error。
- 在结果出站前显式做 repo-relative 路径归一，恢复旧契约里的 `src/lib.rs` 形状，而不是把 `./src/lib.rs` 透给上层 CLI/MCP/Responses。
- `rg` 调用统一 `env_remove("RIPGREP_CONFIG_PATH")`，让 native-tldr 的全文检索不再受用户 ripgreprc 影响。
- `indexed_files` 仍需要单独一次 `rg --files`，因为 `rg --json` 的 begin/end/summary 只覆盖命中文件；但这次把统计实现改成流式读取 stdout 计数 `\0`，避免再用 `Command::output()` 把全量文件列表整块读入内存。
- 测试要分别锁住：
  - `lines.bytes`
  - `path.bytes`
  - repo-relative 路径形状
  - fallback walker 在缺失 `rg` 时仍能跑
  - `RIPGREP_CONFIG_PATH` 已被隔离

## 结论
- 把外部成熟搜索器接进产品链路时，不能只关注“更快”或“更流式”；必须同时核对输出契约、编码分支和环境隔离，否则很容易把性能修复变成行为回归。
- `SearchResponse.indexed_files` 这类看似简单的统计字段，如果上游工具的事件模型不提供同义信息，就不要强行假设单遍可得；应明确保留二次统计，并把实现做成流式、低内存。
- 对工具型能力，测试不能只锁 happy path。凡是底层依赖会根据文件编码、路径编码或用户环境切换输出分支时，都要把这些分支直接写成回归测试。

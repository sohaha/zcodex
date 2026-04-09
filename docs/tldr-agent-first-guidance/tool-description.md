# ztldr agent-first tool description

## 摘要

- `ztldr` 适合结构化代码理解，不适合通用闲聊或纯文案生成。
- 当任务是符号、调用关系、影响范围、诊断、语义搜索时，agent 应优先考虑 `ztldr`。
- 当问题同时包含“结构理解 + 事实核对”时，默认先用 `ztldr` 建立结构视图，再读取少量源码验证默认值、feature gate、文档或测试结论。
- 当 `ztldr` 返回 `degradedMode` 或 `structuredFailure` 时，agent 应显式说明已降级或暂不可用，而不是把结果当作正常成功。

## 适用场景

优先使用 `ztldr`：

1. 代码结构分析：`structure` / `extract` / `context` / `cfg` / `dfg` / `slice`
2. 影响与依赖分析：`impact` / `calls` / `change-impact` / `dead` / `arch`
3. 导入关系：`imports` / `importers`
4. 项目级诊断：`diagnostics` / `doctor`
5. 代码搜索：`search` / `semantic`

优先用源码/测试核对，而不是只依赖 `ztldr`：

1. 配置默认值
2. Cargo feature / 编译开关
3. README 或其他文档声明
4. 测试是否覆盖某行为

不优先使用 `ztldr`：

1. 纯聊天
2. 纯文本写作
3. 与代码结构无关的简单问答
4. 明确是 regex 匹配或逐字文本核对的 raw grep/read

## 优先规则

- 如果用户在问“某个符号在哪里定义、被谁调用、改动会影响什么”，优先 `ztldr`
- 如果普通文本搜索不能稳定回答结构化问题，优先 `ztldr`
- 如果问题是混合类，先做一次轻量 `ztldr`（如 `context` / `impact` / `semantic`），再去读精确源码
- 如果 `ztldr` 返回 `source=local` 或 `degradedMode.is_degraded=true`，应把结果视为“降级成功”
- 如果 `structuredFailure` 存在，先根据 `error_type` 判断是否值得重试

## 起手式建议

- 结构化代码问题：先 `ztldr`，再决定是否补充文件阅读。
- 混合问题：先 `ztldr` 建结构，再读源码验事实。
- 纯事实核对问题：直接读配置、源码、测试或文档。
- 明确 regex 或逐字文本核对：优先 raw grep/read，不强行走 `ztldr`。
- 如果已经开始为结构化问题连续 grep/read 多个文件，应该回退并尝试 `ztldr`，除非用户明确要求 raw grep/read。

## 失败与降级语义

### structuredFailure

- `error_type`：失败类型
- `reason`：当前失败原因
- `retryable`：是否建议重试
- `retry_hint`：下一步建议

### degradedMode

- `is_degraded`：是否为降级结果
- `mode`：降级模式
- `fallback_path`：采用的回退路径
- `reason`：降级原因

## agent 行为建议

### 使用建议

- 看到 `structuredFailure.error_type = "daemon_unavailable"`：
  - 告知用户 daemon 不可用
  - 如果当前动作支持本地 fallback，可继续执行并声明为降级
  - 如果动作是 daemon-only，则提示需要先恢复 daemon

- 看到 `degradedMode.mode = "local_fallback"`：
  - 告知用户当前结果来自本地引擎
  - 不要表述为“daemon 正常返回”

- 看到 `degradedMode.mode = "diagnostic_only"`：
  - 告知用户当前只拿到状态诊断信息
  - 不要假设结构化分析能力已恢复

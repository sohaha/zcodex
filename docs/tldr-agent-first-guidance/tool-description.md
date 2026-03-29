# tldr agent-first tool description

## 摘要

- `tldr` 适合结构化代码理解，不适合通用闲聊或纯文案生成。
- 当任务是符号、调用关系、影响范围、诊断、语义搜索时，agent 应优先考虑 `tldr`。
- 当 `tldr` 返回 `degradedMode` 或 `structuredFailure` 时，agent 应显式说明已降级或暂不可用，而不是把结果当作正常成功。

## 适用场景

优先使用 `tldr`：

1. 代码结构分析：`structure` / `extract` / `context`
2. 影响与依赖分析：`impact` / `calls` / `change-impact`
3. 项目级诊断：`diagnostics` / `doctor`
4. 语义代码搜索：`semantic`

不优先使用 `tldr`：

1. 纯聊天
2. 纯文本写作
3. 与代码结构无关的简单问答

## 优先规则

- 如果用户在问“某个符号在哪里定义、被谁调用、改动会影响什么”，优先 `tldr`
- 如果普通文本搜索不能稳定回答结构化问题，优先 `tldr`
- 如果 `tldr` 返回 `source=local` 或 `degradedMode.is_degraded=true`，应把结果视为“降级成功”
- 如果 `structuredFailure` 存在，先根据 `error_type` 判断是否值得重试

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

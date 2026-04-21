# spawn_agent 使用文档

`spawn_agent` 用于创建一个新的子 agent，把边界清晰、可并行推进的任务委派出去，并返回后续协作所需的 agent 标识。

## 适用场景

- 用户明确要求使用子 agent、委派执行，或并行处理多个独立任务。
- 你已经确认要委派的是非关键路径或可并行的子任务。
- 子任务足够具体、边界清楚、可以单独交付结果。

## 不适用场景

- 用户只是要求“更深入”“更仔细”“帮我研究一下”，但没有明确授权使用子 agent。
- 你接下来的动作必须立刻依赖该结果，此时通常应由当前 agent 直接处理。
- 子任务定义模糊、范围过大，或和主流程高度耦合。

## 调用规则

- 只有在用户明确允许子 agent / delegation / parallel agent work 时才应调用。
- 调用前先做一个简短的高层规划，分清关键路径任务和可并行任务。
- 不要把同一件未完成的事情重复交给多个子 agent。
- 委派后优先继续做本地的非重叠工作，不要反复立刻 `wait`。

## 参数

| 参数 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `message` | `string` | 二选一 | 子 agent 的初始纯文本任务。与 `items` 互斥。 |
| `items` | `array` | 二选一 | 结构化输入项。适合显式传入图片、技能、mention 等。与 `message` 互斥。 |
| `agent_type` | `string` | 否 | 子 agent 角色名。未传时使用 `default`。 |
| `fork_context` | `boolean` | 否 | 为 `true` 时，先复制当前线程历史，再把初始任务发给新 agent。只有确实需要完整继承当前上下文时才开启。 |
| `provider` | `string` | 否 | 覆盖子 agent 的 provider。未同时传 `model` 时，会改用该 provider 的默认模型决议。 |
| `model` | `string` | 否 | 覆盖子 agent 的模型。会替换继承来的模型。 |
| `reasoning_effort` | `string` | 否 | 覆盖子 agent 的推理强度。会替换继承来的推理强度，但必须是目标模型支持的值。 |

## `message` 与 `items`

- 必须二选一。
- `message` 不能为空字符串，也不能只包含空白。
- `items` 不能为空数组。

### `items` 支持的输入类型

| `type` | 关键字段 | 说明 |
| --- | --- | --- |
| `text` | `text` | 纯文本输入。 |
| `image` | `image_url` | 远程图片 URL。 |
| `local_image` | `path` | 本地图片路径。 |
| `skill` | `name`, `path` | 显式传入技能。 |
| `mention` | `name`, `path` | 显式 mention 某个资源，例如 `app://...` 或 `plugin://...`。 |

## 返回值

成功时返回：

```json
{
  "agent_id": "thread-id",
  "nickname": "可选昵称"
}
```

- `agent_id`：后续 `send_input`、`resume_agent`、`wait`、`close_agent` 使用的目标 id。
- `nickname`：用户可见昵称，可能为 `null`。

## 上下文与配置继承

新建子 agent 时，默认会从当前 turn 继承有效配置，并保留运行时环境，包括：

- 当前 provider
- approval policy
- sandbox 设置
- 当前工作目录
- 当前模型与推理强度（除非被 `provider` / `model` / `reasoning_effort` 覆盖）

之后才会叠加 `agent_type` 对应的角色配置。

### `fork_context`

- `false`：子 agent 只收到本次传入的 `message` 或 `items`。
- `true`：子 agent 会先加载当前线程历史，再接收新的初始任务。

如果你只是交代一个独立子任务，通常保持 `false` 更好；只有确实需要子 agent 拥有与当前线程几乎相同的上下文时，才使用 `true`。

## `agent_type`

当前内置角色包括：

- `default`：默认子 agent。
- `explorer`：适合回答明确、范围收敛的代码库问题；适用于快速探索和信息收集。
- `worker`：适合执行型任务，例如实现一部分功能、修复 bug、或拆分大型改动。

仓库也可以通过配置提供自定义角色。

### 角色使用建议

- `explorer`：用于“查清楚 X 在哪里实现”“确认 Y 的数据流”“找出 Z 的调用路径”这类读多写少的问题。
- `worker`：用于明确的实现任务，并且要清楚指定文件范围或职责边界，避免和其他 agent 改同一批文件。

某些角色可能锁定模型或推理强度；这类角色会在工具描述中明确说明，此时你传入的 `model` 或 `reasoning_effort` 不能覆盖它。

## 模型与推理强度

- `provider` 必须是当前环境已配置的 provider 名称，否则会报错。
- `model` 必须是当前环境可用的模型名，否则会报错。
- `reasoning_effort` 必须被目标模型支持，否则会报错。
- 常见取值包括 `none`、`minimal`、`low`、`medium`、`high`、`xhigh`，但具体以目标模型支持列表为准。

- 如果只传 `provider`、不传 `model`，会改用该 provider 的默认模型决议；如果 provider 自己固定了 model，会直接切到那个 model。
- 如果只传 `reasoning_effort`、不传 `model`，会在当前模型上校验。
- 如果同时传 `provider` 和 `reasoning_effort`、但没有传 `model`，会在 provider 解析出的默认模型上校验；若 provider 无法解析默认模型，则需要显式传 `model`。

## 常见工作流

### 1. 启动子 agent

```json
{
  "message": "检查这个仓库里 spawn_agent 的实现位置，并总结关键入口文件。",
  "agent_type": "explorer"
}
```

### 2. 需要完整上下文时启动

```json
{
  "message": "基于当前讨论内容，继续补完文档并给出修改建议。",
  "agent_type": "worker",
  "fork_context": true
}
```

### 3. 使用结构化输入

```json
{
  "items": [
    {
      "type": "text",
      "text": "分析这个 connector 的调用方式"
    },
    {
      "type": "mention",
      "name": "drive",
      "path": "app://drive"
    }
  ],
  "agent_type": "explorer"
}
```

## 与其他协作工具的配合

`spawn_agent` 只是开始，常见配套流程如下：

1. `spawn_agent`：创建子 agent，拿到 `agent_id`
2. `send_input`：继续给该 agent 发送任务或补充上下文
3. `wait`：在确实需要结果时等待一个或多个 agent 完成
4. `resume_agent`：恢复已关闭的 agent
5. `close_agent`：不再需要时关闭 agent

推荐模式：

- 先 `spawn_agent`
- 主 agent 继续做本地工作
- 只有在结果成为关键路径阻塞时才 `wait`
- 收到结果后快速 review，再整合回主流程

## 失败与报错

常见错误包括：

- 同时提供了 `message` 和 `items`
- `message` 为空
- `items` 为空
- `agent_id` 非法（后续协作调用时）
- `model` 不存在
- `provider` 不存在
- `reasoning_effort` 不被目标模型支持
- 超过允许的 agent 深度限制
- 协作管理器不可用或目标 agent 已关闭

## App Server 事件

在 app-server 中，`spawn_agent` 会体现为 `collabToolCall` item：

- `tool = "spawn_agent"`
- `status` 为 `inProgress`、`completed` 或 `failed`
- 常见字段包括 `senderThreadId`、`newThreadId`、`prompt`、`agentStatus`

如果你在实现 UI 或客户端，可以结合 `item/started`、`item/completed` 与 `collabToolCall` 的最终状态来展示子 agent 的创建过程。

## 最佳实践

- 只委派明确、可验证、可独立交付的子任务。
- 给子 agent 的提示要具体，最好直接写清目标输出。
- 编码任务优先拆成不重叠的文件责任范围。
- 子 agent 返回后先 review，再合并其结果。
- 不要把“等待”当成默认动作；能并行就并行。

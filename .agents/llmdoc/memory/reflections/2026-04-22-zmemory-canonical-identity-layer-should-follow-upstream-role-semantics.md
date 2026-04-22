# zmemory canonical identity layer should follow upstream role semantics

这轮 `zmemory` content governance 修复里，最初把 `core://agent` 和 `core://my_user` 收窄成了“助手自称槽”和“用户称呼槽”。这个方向能修掉“任意引号文本被误写成身份值”的问题，但语义模型本身错了：上游 `nocturne_memory` 把这两个路径当成更宽的 identity / personality / bond / preference anchor，而不是固定字段。

直接证据来自上游仓库：

- README 把 `core://agent` 描述成 AI 的 personality / memories / identity 树根。
- README 把 `core://my_user` 描述成 user bond，而不是单一称呼。
- `CORE_MEMORY_URIS=core://agent,core://my_user,core://agent/my_user` 只是常见 boot anchor 组合，不是固定三槽 schema。

因此本地实现应遵守两个层次：

1. canonical identity layer 的路径语义要按“角色”建模，而不是按“固定名字槽”建模。
2. 当前本地内置 content governance 只能声明自己覆盖了哪些路径，不能把这种本地规则反向宣称成上游唯一规范。

这次安全收口方式是：

- `core://agent` / `core://my_user` 退出 `zmemory` 的内置内容治理。
- 仅保留当前本地已证明合理的 `core://agent/my_user` 协作契约治理。
- `codex-core` 的 canonical contract 文案保持宽语义，不在这轮顺手扩改 proactive capture。

后续如果要增强上层自动捕获，应单开一轮设计 richer identity capture，而不是继续把 canonical URI 收窄成名字/称呼槽。

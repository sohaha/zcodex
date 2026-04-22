# zmemory proactive capture should separate identity anchors from addressing keywords

这轮对齐上游 `nocturne_memory` 之后，`core://agent` / `core://my_user` 的契约已经从“名字/称呼槽”放宽成了 identity / personality / preference anchor，但 `codex-core` 里的 proactive capture / recall 仍然停留在旧模型：只会提取名字、自称、称呼，召回则主要依赖“继续按上次方式”这类 continuation 关键词。

这会造成两个实际问题：

1. 用户给 agent 定义 richer identity，例如“你是专业的架构师”，不会被 durable capture。
2. 用户后续引用“这个身份 / 这个角色”时，即使 canonical URI 已经有内容，也可能因为没命中旧的 continuation 关键词而不 recall。

这次修复的正确收口不是把 canonical URI 再收窄回名字槽，而是把上层逻辑拆成两层：

- identity anchor capture：显式提取名字、称呼、稳定身份描述，并把它们合并写回 `core://agent` / `core://my_user`
- collaboration recall trigger：继续保留“按上次方式”这类 contract continuation 触发，但不要让它独占 recall 路由；显式身份引用和“按这个身份/角色继续”也要能拉起 identity layer

同时要避免过宽关键词带来的误召回。例如单独的“继续按”会把“继续按这个身份”误判成通用 collaboration continuation，导致无关的 user-address memory 被一起召回。应优先使用更具体的 continuation / identity-reference pattern，必要时把 identity recall 与 collaboration recall 分开建模。

验证上，至少要同时覆盖：

- 纯函数级提取与 merge 行为
- session/turn 级 recall note 是否真的带出 rich identity anchor
- 与既有 collaboration contract recall 的兼容性

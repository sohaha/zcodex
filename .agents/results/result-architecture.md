status: completed

recommendation_summary:
- 这次判断应修正为：agent 确实用了 `ztldr`，但主要停留在 `search` 层，未把它当成结构分析器来驱动架构判断；`semantic` 还因缺少 `language` 直接失败。
- 在当前仓库，这种“工具已调用但没进入正确决策层”的现象，与仓库里长期出现的“实现存在、共享 runtime 半接线、tests/all 未聚合、prompt/runtime/state 跨层漂移”是同一类系统病。
- 因此核心改进点不是“要求大家多用工具”，而是把分析流程、接线检查和测试聚合做成硬门禁，避免多子代理继续用补丁方式碰运气。

tradeoffs:
- 轻量流程方案成本低、能快速落地，但对执行纪律依赖高。
- 强门禁方案会增加开发前置检查和测试成本，但能显著减少半接线和跨层漂移反复出现。

risks:
- 如果继续允许 agent 只做文本命中、不做结构归因，架构讨论会反复把“局部存在”误判成“系统已接通”。
- 如果不把共享 runtime seam、tool registry plan 和 tests/all 聚合作为同一个验收单元，补丁式开发会继续制造假完成。

validation_steps:
- 已读取 `.agents/llmdoc/architecture/runtime-surfaces.md`、`.agents/llmdoc/architecture/rust-workspace-map.md`。
- 已核对两份直接相关反思：`2026-04-19-local-analysis-tools-must-be-wired-in-tool-plan-and-all-rs.md`、`2026-04-19-zmemory-ztldr-half-wired-build-warnings.md`。
- 已纳入已知事实：先前 agent 对 `ztldr` 的使用以 `search` 为主，`semantic` 因缺少 `language` 发生 `structuredFailure`。

artifacts_created:
- `.agents/results/result-architecture.md`

updated_conclusions:
1. 问题不该表述为“没用 ztldr”，而应表述为“用了 ztldr，但只把它当索引搜索器，没有把结构分析结果接入决策”。这比完全没用更危险，因为它会制造“已做过架构检查”的错觉。
2. `semantic` 缺少 `language` 就直接 `structuredFailure`，说明当前多代理流程里缺少“工具调用前参数成形”的前置校验；这不是单次失误，而是流程层缺口。
3. 当前仓库的真实故障模式，不是功能文件不存在，而是共享 seam 没收口：`tool_registry_plan.rs`、prompt/context 注入、runtime dispatch、`tests/all` 聚合经常不同步。只靠文本搜索无法证明这些 seam 已闭合。
4. llmdoc 反思已经反复证明：这里的补丁式开发之所以失效，是因为“crate/CLI 还在”与“生产链路可达”之间有明显断层。`ztldr` 若只用于找文件，恰好会放大这种误判。
5. 对这个仓库，正确的架构分析顺序应是：先用 `ztldr` 做结构入口和调用链定位，再核对共享 runtime 接线点，最后看 `tests/all` 是否真正把能力编译并执行。任何只停在搜索命中的结论都不够。
6. 因而最需要的不是再加一层说明文档，而是把“结构分析 -> seam 检查 -> 聚合测试”固化成多子代理的决策门；否则 agent 之间只会继续交替制造局部补丁和全局漂移。

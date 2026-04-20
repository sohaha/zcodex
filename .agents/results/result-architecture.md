status: completed
recommendation_summary: |
  ztldr 在 core/cli/mcp 三条入口存在明确的语义分叉：CLI 将 daemon 不可用时的本地回退显式暴露为 source=local/message=fallback；MCP 对 daemon-only 动作显式返回 structuredFailure/is_error；core 则复用可回退执行路径，但主要把 degraded 信息写入内部上下文而不是抬升为显式失败信号。综合判断，这不是纯粹预期设计，而是“共享底层 + 入口语义未统一”的设计折中，且在 core 入口已形成用户可感知缺陷：会制造令人困惑的静默降级。
tradeoffs: |
  CLI 偏向可用性，允许本地引擎兜底并提示 fallback；MCP 偏向协议显式性，保留 structuredFailure/degradedMode；core 偏向对话连续性，但代价是把关键运行时状态隐入内部上下文，外部只看到工具正常返回。
risks: |
  1. 模型在 core 中可能把 local fallback 误判为正常 daemon 结果。 2. warm/auto-warm 决策会基于一次“成功但降级”的结果继续推进，放大错误心智模型。 3. 三入口测试各自成立，但跨入口契约不一致。
validation_steps: |
  1. 对照 core/src/tools/handlers/tldr.rs 中 maybe_issue_first_structural_warm、run_tldr_handler_with_hooks、extract_degraded_mode。 2. 对照 cli/src/tldr_cmd.rs 中各分析/搜索命令的 source/message fallback 分支。 3. 对照 mcp-server/src/tldr_tool.rs 中 daemon-only 路径的 structuredFailure/is_error 输出。 4. 对照 native-tldr/src/tool_api.rs 与 native-tldr/src/lifecycle.rs 中 query_or_spawn/ready_result 语义。
artifacts_created:
  - .agents/results/result-architecture.md

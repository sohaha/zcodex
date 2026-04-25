# ztldr 结构查询 payload 必须在工具层限幅

## 背景

- 用户反馈出现了“`ztldr` 结构查询超时，后续用精确文件读取继续”这类结果。
- 这类反馈表面上像提示词或模型汇报问题，但实测 `structure` 查询能成功返回，真正异常是符号级结构查询会把大量 `incoming`、`imports`、`references` 和 graph node 明细一起序列化给模型。

## 发现

- `native-tldr/src/analysis.rs::build_analysis_detail` 之前只限制了 `units` 和 `edges`，没有限制 `nodes`、`symbol_index` 以及单个 unit 的关系列表。
- 在大仓库中查询高 fan-in 符号时，即使 summary 很短，JSON payload 仍可能被 `called_by`、imports、references 和 dependency 明细撑大，造成工具调用慢、输出截断或模型把结果概括成“结构查询超时后退回精确读取”。
- 修提示词不能解决根因；必须先控制结构化数据体大小，同时保留 `overview` 的真实总量计数，让调用方知道结果被限幅但规模信息仍可信。

## 结论

- 以后处理 `ztldr` “超时 / 结果太大 / 模型退回 raw read”类问题时，除了检查 daemon 状态和 tool description，还要检查结构化 payload 的字段级限幅。
- 对 `AnalysisDetail` 这类模型可见结构化结果，必须同时限制集合数量和单项内部关系数量；只限制顶层 matches/units 不够。
- 限幅后应把 `truncated` 标成 `true`，并用测试覆盖高 fan-in 符号，确保 overview 计数保留真实规模。

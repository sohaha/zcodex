# ztok 决策 trace 一旦接进多条压缩路径，验证覆盖也必须按接线面同步扩齐

## 背景

2026-04-21 在推进 `ztok` 下一阶段路线图的 `a3` 时，我先把压缩决策 trace 接到了 `read/json/log/summary` 四条共享压缩主路径，并新增 `--trace-decisions` 用统一 runtime settings payload 显式开启 `stderr` JSON 输出。

第一轮集成测试只锁了 `ztok read`。实现本身没问题，但 issue 审查很快暴露出一个完成度缺口：既然生产代码已经把 `json/log/summary` 都接进统一 trace，就不能只靠单一路径的 happy path 证明整项 done。

## 这次确认的做法

- `ztok` 这类“统一接线、多入口复用”的功能，测试覆盖要跟着接线面走；如果 `read/json/log/summary` 都声明复用同一能力，就至少要有集成证据分别锁住这些主路径的开启行为和 `stdout` 不变合同。
- `--trace-decisions` 这类 side channel 能力，验证要成组三件事一起看：
  - 默认关闭时 `stderr` 不应平白多出 trace
  - 显式开启时要看到统一的结构化事件
  - 事件里不能带原始正文或整段原始快照
- `summary` 是特殊入口：它的输入不是文件正文，而是 shell 命令和摘要结果。这里的 trace `source` 不能直接回喷原始命令串，应该复用稳定签名或等价的非敏感标识。

## 为什么值得记

- 只测一条主路径，很容易把“代码已经接了四条路径”和“验证只证明其中一条”混成完成，最后在 Cadence 收口时被 done_when 反查打回。
- `summary` 的调试视图如果直接带原始 shell 命令，虽然没泄露正文快照，仍然可能把命令行上的敏感参数带进 `stderr` side channel。
- 对这类默认关闭、显式开启的开关，如果不同时锁“关闭”和“开启”，后续重构很容易把 trace 默认打开或混进 `stdout` 而不自知。

## 下次复用

- 给 `ztok` 或类似共享底座补新 side channel 时，先列出所有已经接线的生产入口，再按入口补最小集成验证，不要等审查阶段才补覆盖面。
- 如果 trace / debug 事件需要携带 `source`，优先用稳定标识、摘要或 hash，不要默认塞原始命令或原始输入片段。
- Cadence issue 的 done_when 只要写了多个入口，就把“每个入口至少一条证据”当成默认完成线，而不是额外加分项。

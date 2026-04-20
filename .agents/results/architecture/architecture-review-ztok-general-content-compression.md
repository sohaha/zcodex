# ztok general content compression delta review

status: pass
method: recommendation_mode
decision_scope: delta review for a3/a5 verifiability and a5 sequencing

## Architecture Problem

本次只检查上轮指出的两个 Execution 边界问题是否已消除：
- `a3` 的“可调边界”是否已经变成可审计、可验证的完成标准
- `a5` 的治理收口是否已经有足够的语义校验入口，而不再只是关键词存在性检查

并复核 `a5` 继续放在 `a4` 之后是否仍然合理。

## Delta Assessment

### a3

上轮问题已消除。

当前 `a3` 已把“可调”从抽象要求收紧到三种可审计暴露面之一：
- 配置项
- 构造参数
- 测试 seam

同时 `done_when` 与 `validate_by` 共同锁定了四类关键场景：
- 高相似命中
- 低置信度回退
- 候选冲突回退
- 差分不可读回退

这使 Execution 不再只能凭实现者解释“这个常量以后能改”，而是可以据此判断是否真的存在可验证边界。

### a5

上轮问题也已消除。

当前 `a5` 不再只依赖 `rg` 检查词项，而是先要求：
- 手动核对 `.version/sqz.toml` 的 `source / ref / commit hash / integration mode` 是否与本轮实际参考上游一致
- 手动核对 `upgrade-rtk` 对双上游统一入口的描述是否超出 `a4` 实际落地范围

之后才用 `rg` 做存在性确认。这个顺序把“语义正确”放到了“文本存在”之前，足以支撑 Execution 判断 done。

## Sequencing Comparison

### Option A: `a5` 继续在 `a4` 之后

优点：
- 基线记录与 skill 收口可以按 `a4` 最终实际落地范围冻结
- 避免在命令接入与 integration mode 尚未稳定前过早固化治理口径

成本：
- 治理资产出现稍晚

### Option B: 将 `a5` 前移到 `a4` 之前

优点：
- 更早形成文档资产

成本：
- 一旦 `a4` 接入面或边界调整，`.version/sqz.toml` 与 `upgrade-rtk` 说明容易返工
- 文档治理与实现真实边界更容易漂移

结论：继续选择 Option A 仍然更合理。

## Recommendation

`a3` 与 `a5` 的可验证性问题已经消除，`a5` 放在 `a4` 后面仍然合理，本轮 delta review 结论为 PASS。

## Risks

- `a3` 的 `validate_by` 仍含自然语言说明，Execution 时需要实际按说明回读测试，而不是只执行命令
- `a5` 的手动核对步骤依旧依赖执行者自觉，但这已经属于可接受的治理审查成本，不再是结构性缺口

## Validation Steps

1. 执行 `a3` 的 crate 测试后，回读相关测试用例，确认四类场景与“非私有常量”约束都被覆盖。
2. 执行 `a5` 时，先人工核对 `.version/sqz.toml` 与 `upgrade-rtk` 描述，再运行 issue 中的 `rg` 命令做存在性确认。
3. 保持 `a5 -> depends_on = ["a4"]` 不变。

## Artifacts Created

- `.agents/results/architecture/architecture-review-ztok-general-content-compression.md`
- `.agents/results/result-architecture.md`

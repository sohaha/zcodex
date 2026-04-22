# zmemory 内容治理应先落 service 规则层，并把 workspace 基线阻塞与功能改动分开

## 场景
- 任务是为 `codex-zmemory` 建立“单节点内容内部互斥事实”的底层治理框架，首轮只做 framework / contract / sample rules，不提前把 `create` / `update` / `doctor` / `review` 全量接线到同一轮里。

## 结论
- 内容治理问题要先定义为 `service` 层的可复用规则执行入口，而不是继续在 `codex-core` 的 recall / proactive capture 里追加 merge/replace 补丁。
- 首轮框架只需要解决三件事：URI 作用域匹配、规范化结果建模、冲突显式建模。把这些收敛成独立 module 后，后续写入路径和诊断路径只复用同一个入口即可。
- 以 canonical identity URI 作为首批样本时，规则也要按 registry/module 形式落地，而不是把 `core://agent`、`core://my_user`、`core://agent/my_user` 的字符串判断散在多个 action 里。
- 本地验证若被 workspace 缺件阻塞，必须把“仓库基线坏了”和“本次改动未完成”拆开记录；这次 `cargo test -p codex-zmemory`、`just fmt`、`just fix -p codex-zmemory` 都在解析 workspace 时被缺失的 `codex-rs/federation-protocol/Cargo.toml` 阻断，不能把这个基线问题误记成治理框架本身回归。

## 这次落地
- 在 `codex-rs/zmemory/src/service/governance.rs` 建了独立治理模块，用规则表承载 canonical identity URI 的作用域与规则。
- 在 `codex-rs/zmemory/src/service/contracts.rs` 新增治理结果 contract，让后续写入、doctor、review 共享同一结果模型。
- 在 `codex-rs/zmemory/src/service/tests.rs` 增加 canonical URI 的 normalize/conflict 样例测试，证明框架不是单一补丁。

## 后续提醒
- 第二阶段接写入路径时，应直接消费治理结果里的 `status/changed/governed_content/issues`，不要重新发明一套局部返回值。
- `import` 和批量动作应继续复用 `create_action_in_tx` / `update_action_in_tx` 暴露出来的治理结果，只做事务编排和汇总，不要在外层再拼一套近似 contract。
- 第三阶段扩 `doctor/review` 时，也应复用同一治理入口做“检测”，不要在诊断层重写一遍冲突判断。

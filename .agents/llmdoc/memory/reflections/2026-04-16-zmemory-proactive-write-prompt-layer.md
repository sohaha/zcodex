# zmemory 主动写入先收敛到提示注入层

## 现象
- 用户反馈 `zmemory` “不会主动写入记忆”，表现为会读 `system://boot`，但在对话里很少主动调用 `create_memory` / `update_memory`。
- 现有实现里，代码层只有 `core/src/memories/zmemory_preferences.rs` 对少量“稳定偏好”场景做了主动写回；大部分 durable memory 仍依赖模型自己判断是否要写。

## 根因
- `zmemory` 工具本身具备读写能力，缺口主要不在 `codex-rs/zmemory` 服务层，而在注入给模型的 developer instructions 不够强。
- 旧版 `core/templates/zmemory/write_path.md` 虽然要求新会话先 `read_memory("system://boot")`，也提到了 durable knowledge 要写入，但没有把“主动写入是默认动作”说清楚。
- 模型因此更容易把写记忆当成可选动作，而不是和回答、修正同等优先的默认职责。

## 修复
- 在 `core/templates/zmemory/write_path.md` 中新增三组强约束：
  - `Write-now defaults`：把 durable 决策、根因、复用结论、稳定偏好明确列为立即 `create_memory` / `update_memory` 的触发条件。
  - “口头表态不等于落笔”规则：明确禁止在未实际写入时声称“我记住了”。
  - `Maintenance while recalling`：读取记忆节点时顺手修过时、重复和 disclosure 缺陷，避免只读不维护。
- 在 `core/src/memories/prompts_tests.rs` 增加对应断言，锁住这组提示词 contract。

## 经验
- 当用户反馈“模型不会主动做 X”，先检查 injected prompt / developer instructions 是否把 X 定义成默认职责；不要第一反应就去改工具服务层。
- 对 `zmemory` 这类“能力已存在、是否调用取决于模型策略”的系统，提示词 contract 往往比底层 API 扩展更关键。

## 验证边界
- 本次只修改提示注入层和测试断言，没有改 `codex-rs/zmemory` 的数据库或系统视图逻辑。
- 本地 `cargo test -p codex-core ...` 未能完整跑通，阻塞来自工作区现有 `native-tldr` 脏改动导致 `codex-core` 编译失败（`TldrToolCallParam` 缺少 `match_mode` 字段），不是本次改动直接引入。

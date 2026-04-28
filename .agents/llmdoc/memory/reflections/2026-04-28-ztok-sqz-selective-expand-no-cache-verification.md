# ztok selective sqz 行为验证要隔离目标目录与工作区噪音

## 触发
- 对齐上游 `sqz` 的可用行为时，本地只选择性吸收 `cache expand` 与临时禁用 session dedup，而不是推进完整 `sqz` parity。
- 当前工作区同时存在无关 `context_hooks`、app-server/config 和长期 Cargo 进程，常规验证容易被共享 `CARGO_TARGET_DIR` 锁竞争或无关编译失败干扰。

## 经验
- `ztok` 采用 `sqz` 思路时，应同步更新三类事实：运行时行为、Embedded ZTOK 提示词、`.version/sqz.toml` selective-reference 基线。只改 runtime 会让模型仍不知道如何展开 `[ztok dedup <ref>]`。
- `cache expand` 的验证应覆盖原始 snapshot、压缩 output、缺失 ref 与前缀歧义；`--no-cache` 应覆盖 CLI flag 和 `CODEX_ZTOK_NO_DEDUP=1` 两条入口，并断言压缩仍保留、dedup marker 消失。
- 在 dirty monorepo 中不要机械跑全仓 `just fmt` 或共享 target 的大测试；先用包级 `cargo fmt --package ...` 和独立 `CARGO_TARGET_DIR` 跑定向测试，避免格式化/锁竞争污染无关工作。
- core prompt 单测若因无关配置字段漂移（例如 `Config` 新增字段但测试构造体未补齐）编译失败，应把它记录为验证阻塞，不要顺手修不属于当前任务的配置链。

## 下次做法
- 先确认 `.version/sqz.toml` 是否需要推进；如果推进，最终汇报必须明确 `integration_mode = "selective-reference"`，避免暗示完整上游 parity。
- 对 `ztok` 提示词变更，至少用 `prompts_tests` 锁住 `cache expand`、`--no-cache` 和 `CODEX_ZTOK_NO_DEDUP=1` 三个模型可见锚点。
- 大量后台 Cargo 任务存在时，用 `CARGO_TARGET_DIR=/tmp/codex-target-<scope>` 复跑受影响测试；共享 target 超时不应被误判为功能失败。

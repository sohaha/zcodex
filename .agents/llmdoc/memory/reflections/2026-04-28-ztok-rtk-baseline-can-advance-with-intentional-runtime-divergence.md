# ZTOK RTK 基线推进可以保留有意分叉的运行时行为

## 触发

执行 `upgrade-rtk` 时，上游 `rtk-ai/rtk` 从记录的 `v0.37.1` 推进到 `v0.37.2`。本地需要判断是否同步运行时代码，还是仅推进 `.version/rtk.toml`。

## 经验

- 先读 `.version/rtk.toml`、`.version/sqz.toml` 和本地 `ztok` 入口，再看上游 diff；不能只因为有新 tag 就覆盖本地实现。
- `v0.37.2` 的主要变更集中在上游 hook/discover/meta 面；这些不属于 Codex curated embedded surface。
- 上游 `curl` 从 JSON schema 压缩改为简单截断加 tee hint，但本地 `ztok curl` 已经接入 fetcher compression、内部 URL JSON 保真、session dedup 和 trace redaction；这是本仓库基于 `sqz` 选择性压缩面的有意分叉。
- 这种情况下可以推进 RTK 记录基线，但不要同步会削弱本地压缩合同的上游简化实现，也不要推进 `sqz` 基线，除非实际审计并采用了新的 compression/dedup 行为。

## 下次做法

- 对 RTK 升级先用 `git ls-remote` 确认最新 tag/commit，再用上游 diff 判断是否命中 curated commands、prompt、alias 或过滤行为。
- 若只更新 baseline，验证应收敛到 `.version/*` 与 skill/checklist 文案；不要运行 runtime 测试来制造无关成本。
- 若发现上游行为与本地压缩面冲突，优先记录“已审计并有意保留分叉”，再只更新实际同步过的基线字段。

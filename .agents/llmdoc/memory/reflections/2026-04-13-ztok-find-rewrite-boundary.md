# ztok find 自动重写边界反思

## 背景

`codex-rs/ztok/src/rewrite.rs` 之前把 `find` 放在 `DIRECT_PREFIXES` 里，导致普通 shell `find ...` 会被无条件改写成 `ztok find ...`。  
但 `codex-rs/ztok/src/find_cmd.rs` 运行时又会拒绝 `-o`、`-not`、`-exec` 等复合谓词或动作参数。

结果是：原本系统 `find` 可以成功执行的复杂命令，会因为自动重写而失败。

## 这次学到的事

- shell 自动重写属于“优化显示/体验”的一层，不能改变原生命令本来的成功/失败语义。
- 如果某个命令在 rewrite 层和运行时都要维护能力边界，必须共用同一事实源；否则一边放行、一边报错，很容易出现语义倒挂。
- 对这类问题，优先修正 rewrite 判定，让“不支持”的形态直接 passthrough 到原生命令；不要先改写再依赖运行时错误提示兜底。

## 本次落地

- 把 `find` 从 `DIRECT_PREFIXES` 的无条件改写列表中移出。
- 给 `find` 增加专用 `rewrite_find(...)`，在 rewrite 阶段直接复用 `find_cmd` 的 `UNSUPPORTED_FIND_FLAGS` / `has_unsupported_find_flags()`。
- 为“简单 `find` 继续改写”和“复杂 `find` 保持 passthrough”都补了测试。

## 后续提醒

- 以后给 shell 自动重写新增命令时，先确认该命令是否存在“支持子集”；如果有，就不要直接放进无条件直通列表。
- 若运行时已有“不支持参数集合”或等效能力边界，rewrite 层应优先复用，而不是复制一份列表。

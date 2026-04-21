# 2026-04-21 ztok 双上游基线必须区分 curated RTK sync 与 selective sqz reference

## 背景

- `ztok` 现在同时受两条上游参考链影响：
  - `RTK` 决定嵌入命令面、alias、帮助输出与 prompt 文案边界。
  - `sqz` 只为通用压缩、会话 dedup、近重复差分这些局部能力提供参考。
- 如果只保留 `.version/rtk.toml`，而把 `sqz` 参考散落在 issue、plan 或临时 handoff 里，后续做同步或审计时很容易发生两类错误：
  - 再补一个平行的 `sqz` sync skill，导致双上游口径分裂。
  - 把一次 selective reference 误写成 `sqz` 全量 upstream parity。

## 结论

- 对这类“一个上游负责命令面、另一个上游只负责局部实现参考”的仓库，应继续保留单一 skill 入口，但要把两条基线显式拆开记录。
- `sqz` 这条链路至少要有独立版本文件，明确：
  - `source`
  - `upstream_ref`
  - `upstream_commit`
  - `integration_mode`
- 当本地只是借鉴压缩/去重思路而不是做真正 selective sync 时，`integration_mode` 不能沿用 `codex-curated` 这类会让人误解成产品面同步的名字；应改成能直观表达“局部参考而非全量对齐”的值，例如 `selective-reference`。
- 如果引用的是 `main` 分支公开文件而不是 tag，`upstream_ref = "main"` 仍然不够；必须同时钉 commit，避免后续 `main` 漂移后无法回到当时实际参考的源码快照。

## 落地做法

- 为 `sqz` 新增独立的 `.version/sqz.toml`，把 `main` + 精确 commit 写成可审计基线。
- 把 `.codex/skills/upgrade-rtk/` 扩成 `RTK + sqz` 双上游统一入口，并在 skill 与 checklist 里显式写出：
  - RTK 管命令面与 CLI 集成
  - sqz 只管当前 repo 真实采用的压缩 / dedup 参考面
- 在 skill 文案里直接禁止把 `sqz` 的 hooks、proxy、plugins、stats、dashboard 等产品面默认纳入同步范围，除非用户显式扩 scope。

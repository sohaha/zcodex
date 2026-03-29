# Review Playbook

## 最小 review 检查清单

1. `codex zmemory stats --json`：读取 `orphanedMemoryCount`、`deprecatedMemoryCount`，判断是否有 review 压力。
2. `codex zmemory doctor --json`：查看 `issues` 里是否有 `orphaned_memories`、`deprecated_memories_awaiting_review`，确认 FTS / keyword / search index 状态。
3. `codex zmemory export recent --json`：确认最近写入的内容已经被系统视图收录，便于决定是否需要 `update`。
4. `codex zmemory export glossary --json`：确认 trigger / keyword 覆盖，判断是否需要 `manage-triggers` 或 `add-alias`。
5. 如需清理：
   - `codex zmemory update <uri>`： patch/append/metadata 收敛
   - `codex zmemory delete-path <uri>`：废弃过时路径
   - `codex zmemory add-alias <alias> <target>`：弥补多语境入口
   - `codex zmemory manage-triggers <uri> --add <keyword>`：强化触发

## 交接指引

- review 完成后，把新的关键结论写入项目主节点。
- 若有长周期治理安排，更新 `.ai/memory/handoff.md` 并邀请 vbm 继续 follow-up。

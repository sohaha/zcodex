# Admin Workflows

适合用于本地治理、导出、doctor 检查与 review 收口。

## 常见治理动作

### 1. 状态盘点

- `codex zmemory stats --json`

### 2. 健康检查

- `codex zmemory doctor --json`

### 3. 系统视图导出

- `codex zmemory export workspace --json`
- `codex zmemory export defaults --json`
- `codex zmemory export boot --json`
- `codex zmemory export index --domain core --json`
- `codex zmemory export paths --domain core --json`
- `codex zmemory export recent --json`
- `codex zmemory export glossary --json`
- `codex zmemory export alias --json`

### 4. 重建搜索索引

- `codex zmemory rebuild-search --json`

## 推荐顺序

1. 先看 `workspace` / `defaults`
2. 再跑 `stats`
3. 再跑 `doctor`
4. 再看 `boot` / `paths` / `recent` / `glossary` / `alias`（其中 `recent` 只反映最近内容版本，不覆盖 alias/trigger/path 元数据治理）
5. 最后根据缺口执行 `update`、`manage-triggers`、`add-alias` 或 `rebuild-search`

## search parity 提醒

- 未知 domain 会返回带 valid domains 的显式错误。
- alias / trigger 查询支持 separator-normalization。
- search 结果会按 node 去重，并按 priority、path 长度、URI 排序。
- snippet 优先 literal，其次 token，再退回内容片段。

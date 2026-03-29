# Usage Modes

当前仓库中的 memory 使用分为三类：

## 1. bootstrap / recall

- 用 `read system://boot` 读取配置化 boot 锚点。
- 已知 URI 用 `read`，未知 URI 用 `search`。

## 2. capture / refine / linking

- 新稳定信息用 `create`。
- 修订旧节点优先 `update`。
- 为了提高召回，补 `add-alias` 与 `manage-triggers`。

## 3. review / governance

- `stats` / `doctor` 看 orphan / deprecated / disclosure / alias pressure。
- `export recent|glossary|alias` 看最近变化与 trigger wiring。
- 必要时 `rebuild-search`。

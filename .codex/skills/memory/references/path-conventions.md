# Path Conventions

推荐的长期路径约定：

## 项目主线

- `core://project_x`
- `core://project_x/constraints`
- `core://project_x/architecture`
- `core://project_x/decisions`
- `core://project_x/open_questions`
- `core://project_x/handoff`

## 用户/关系主线

- `core://agent`
- `core://agent/my_user`
- `core://relationship/history`
- `core://relationship/preferences`

## 使用原则

1. 主节点尽量短、稳定。
2. 约束/决策/问题尽量拆成固定子路径。
3. 能 refine 就不 create 近似新节点。
4. 当同一节点需要从多个语境访问时，用 alias，不复制内容。
5. boot 锚点优先放进 `CORE_MEMORY_URIS`，保证 `system://boot` 输出稳定。

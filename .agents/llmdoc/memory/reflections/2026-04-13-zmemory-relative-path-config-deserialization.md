# zmemory 相对路径不要在配置层提前绝对化

## 现象
- TUI/CLI 启动时，如果在 `config.toml` 里写：

```toml
[zmemory]
path = ".agents/memory.db"
```

- 配置加载会直接失败，报 `AbsolutePathBuf deserialized without a base path`。

## 根因
- `core/src/config/types.rs` 把 `ZmemoryToml.path` 定义成了 `Option<AbsolutePathBuf>`。
- 这会让 `ConfigToml` 在反序列化阶段就要求 `zmemory.path` 已经有绝对路径基准。
- 但 `zmemory` 的真实语义不是“相对 config.toml 目录解析”，而是：
  - git 仓库内相对主 repo root 解析
  - 非 git 目录相对当前 cwd 解析
- 因此配置层在 `zmemory` 专用路径解析器之前就把相对路径拦截掉了。

## 修复
- 把 `ZmemoryToml.path` 改回原始 `PathBuf`。
- 保留相对路径字符串直到 `codex-zmemory::resolve_zmemory_path()`，再按 repo root / cwd 语义解析。
- 补一个 `ConfigBuilder` 级回归测试，确认：
  - `config.toml` 可以加载相对 `zmemory.path`
  - 运行时最终会解析到仓库根下的 `.agents/memory.db`

## 后续规则
- 只要某个配置项的相对路径语义依赖运行时上下文（例如 repo root、turn cwd、workspace base），就不要在通用 `ConfigToml` 反序列化层用 `AbsolutePathBuf` 提前吃掉它。
- `AbsolutePathBuf` 适合“配置文件目录就是解析基准”的字段；不适合需要交给子系统二次决策的字段。

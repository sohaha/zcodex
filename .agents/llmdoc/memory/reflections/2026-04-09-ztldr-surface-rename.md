# 2026-04-09 ztldr 对外命名收敛反思

## 背景
- 任务目标不是简单把 `codex tldr` 改成 `codex ztldr`，还要把 MCP tool 的对外名字一起收敛，并排除 `native-tldr` crate / `.codex/tldr.toml` / `.tldr*` 等底层命名。

## 这次学到的事实
- 当前仓库里 `tldr` 这个词分布在 4 个层次：CLI 子命令、MCP tool 名、native-tldr 内部能力/crate 名、feature/config/artifact 名。
- 真正需要跟着用户-facing surface 一起改的是前两层；后两层如果机械替换，会把 feature gate、crate 边界和项目配置名一并破坏。
- `codex-core` 的 auto-rewrite/read-gate/router/spec 测试会把 tool 名字当成稳定契约；只改 tool 注册名而不改这些测试，`cargo test -p codex-core tldr` 会立刻暴露回归。
- `codex-cli` 的文本输出已经是中文标签；命令名重命名时，相关文本测试不一定失败在命令名本身，也可能暴露出既有断言与当前中文输出不一致。

## 这次踩到的坑
- 一开始只把 CLI 与部分 MCP surface 改到 `ztldr`，遗漏了 `codex-core` 的重写/路由测试断言，导致 `cargo test -p codex-core tldr` 失败。
- 用批量替换更新 mcp-server 测试时，误把 `#[cfg(feature = "tldr")]` 改成了 `ztldr`，说明“对外工具名”和 Cargo feature 名必须明确分开处理。
- 本地环境开始时没有 `cargo-nextest` 和 `just`，需要先按仓库约定执行 `mise run dev-tools`，否则后续验证/收尾命令会直接缺失。

## 后续建议
- 以后再做对外工具命名收敛时，先显式分类：`user-facing surface`、`tool registry/dispatch`、`feature/config/artifact`、`docs/tests`，不要先做全仓字符串替换。
- 对 `codex-core` 这类共享层，优先跑最能覆盖契约的定向测试；这次 `cargo test -p codex-core tldr --quiet` 是最快暴露遗漏的验证入口。
- 若后续继续扩展 `ztldr` 能力，优先保持“外部叫 `ztldr`，内部仍可保留 native-tldr/tldr feature”这条边界稳定，避免再次混淆。

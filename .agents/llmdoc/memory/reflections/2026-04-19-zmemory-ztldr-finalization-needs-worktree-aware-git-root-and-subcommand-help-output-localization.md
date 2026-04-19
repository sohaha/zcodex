# 2026-04-19 zmemory / ztldr 收口需要同时补齐 worktree git 根解析与子命令 help 输出汉化

- 这轮 `zmemory` / `ztldr` 最后没有卡在功能本体，而是卡在两个收口层：
  - `zmemory` 依赖的 `codex-git-utils::resolve_root_git_project_for_trust()` 在同步后退化成“只认普通 `.git` 目录”的版本，导致 worktree 下默认 project key 和主仓不一致。
  - `ztldr` / `zmemory` 的 `--help` 子命令说明仍残留英文，但直接改 clap 自动生成 `help` 子命令内部参数节点会触发测试二进制里的 `Argument 'subcommand' is undefined` panic。

## 结论

- `zmemory` 的 project-scoped 语义不能只在自身 crate 修补；如果根因在共享 git root helper，就必须回到 `git-utils` 修。
- 对 CLI 汉化，优先使用稳定的“渲染后字符串替换”收口，不要依赖 clap 自动生成 `help` 子命令内部结构在所有构建路径都一致。

## 本轮修正

- 在 `codex-rs/git-utils/src/info.rs` 里把同步 trust helper 补回 worktree-aware 行为：
  - 普通 repo 继续返回 `.git` 所在仓库根。
  - `.git` 是 `gitdir:` 指针且落在 `.../.git/worktrees/<name>` 时，返回主仓库根。
- 在 `codex-rs/cli/src/main.rs` 里保留 `help` 子命令 `about` 的本地化，但把真正不稳定的文案收口放回 `localize_help_output()`。
- 为 `codex-cli` 集成测试补了 `ztldr --help` / `zmemory --help` 的回归断言，并兼容帮助文本可能落在 `stdout` 或 `stderr` 的差异。

## 后续规则

- 只要本地功能和 “repo 根” 有关，就不要默认相信同步后的共享 helper 仍保有 worktree 语义；先找共享实现和现有跨 crate 测试。
- 只要 help/localization 改动触碰 clap 自动生成的命令树，就必须同时验证：
  - 主二进制真实输出
  - `codex-cli --bin codex` 单测
  - 对应集成测试里通过 `cargo_bin("codex")` 拉起的测试二进制

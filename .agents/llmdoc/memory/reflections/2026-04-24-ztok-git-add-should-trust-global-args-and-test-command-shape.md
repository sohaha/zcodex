# 2026-04-24 ztok git add 应依赖全局参数而不是显式设置 current_dir

## 背景
- `codex-rs/ztok/src/git.rs` 的 `run_add` 曾被补上一组 `std::env::current_dir()` / `Command::current_dir(...)` 调用，意图是“确保 `git add` 在当前工作目录执行”。
- 继续核对 `ztok` 的 Git 入口后确认，真正决定目标仓库/工作树的是 `git_cmd(global_args)` 注入的 `-C`、`--git-dir`、`--work-tree` 等全局参数，而不是 `run_add` 再手动设置子进程 cwd。
- `Command` 默认就会继承当前进程工作目录，因此这类改动既不改变正常路径行为，还会额外引入 `current_dir()` 失败的新错误面。

## 这轮有效做法
- 移除 `run_add` 中显式获取和设置 cwd 的逻辑，让 `git add` 与后续 `git diff --cached --stat --shortstat` 继续只依赖 `git_cmd(global_args)` 和默认继承的进程 cwd。
- 把 `run_add` 里两段命令构造提炼成可单测的 `build_add_command` 与 `build_cached_diff_stat_command`，直接锁定命令参数形状。
- 在测试里同时断言两件事：
  - 全局参数必须保留在子命令前，确保 `-C` / `--git-dir` / `--work-tree` 语义不回退。
  - `Command::get_current_dir()` 必须为 `None`，避免以后再把冗余 `current_dir` 补回来。

## 关键结论
- 修 `ztok git` 这类 Git wrapper 时，先确认仓库定位语义到底来自 Git 全局参数、默认继承的 cwd，还是调用点额外覆盖；不要把“显式写出来”误当成“行为修复”。
- 若目标是防止仓库解析回归，最有效的回归测试通常不是跑完整 Git 流程，而是直接验证 `Command` 的 program、args 和 current_dir 形状。
- 对 `Command` 来说，新增 `current_dir()` 不是无害重构；它会改变错误模型，并可能掩盖真正的仓库定位真相源。

## 后续建议
- 以后若再改 `ztok` 的 Git 子命令，优先把“命令构造是否正确”抽成可断言单元，再决定是否需要更重的集成测试。
- 遇到“cwd 看起来不对”的症状时，先从 `global_args` 的来源和顺序排查，不要第一反应就是给 `Command` 补 `current_dir()`。

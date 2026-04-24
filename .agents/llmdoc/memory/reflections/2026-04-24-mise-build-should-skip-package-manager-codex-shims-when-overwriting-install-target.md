# `mise run build` 安装目标应跳过 package-manager `codex` shim

## 现象

`mise run build` 构建成功后，用户实际运行的 `/usr/local/bin/codex` 仍然是旧二进制；同时日志显示构建脚本把新产物原子覆盖到了 `~/.local/share/mise/installs/node/.../lib/node_modules/@sohaha/zcodex/bin/codex.js`。

## 根因

`.mise/tasks/build` 之前优先用 `command -v codex` 选“已安装 codex”。当任务运行在 `mise` 环境里时，PATH 里会混入 Node 全局包自带的 `codex` shim，它最终 realpath 到 package-manager 管理的 `codex.js`。脚本于是把 Rust ELF 写进了 npm/mise 维护的 shim 目标，而没有继续寻找真正的用户入口，例如 `/usr/local/bin/codex`。

## 修正

- `find_installed_codex_path()` 不再盲信 `command -v codex`，而是按 PATH 顺序扫描候选。
- 新增 `codex_path_is_managed_node_shim()`，把 realpath 落到 `.../lib/node_modules/@sohaha/zcodex/bin/codex.js` 的候选排除掉。
- `find_writable_user_path_dir()` 也跳过这类受 package manager 管理的目录，避免“没有真实 codex 时”又把安装目标写回 shim 目录。
- 回归测试需要同时覆盖：
  - PATH 里前面是 managed shim、后面是真实 codex 时，应优先覆盖真实入口。
  - 只有 managed shim 时，应回退安装到正常的用户 bin 目录，而不是改写 `codex.js`。

## 经验

像 `mise`、`npm`、`pnpm` 这类运行时会把自身 shim 注入 PATH 的工具，不能把“PATH 里第一个命令解析结果”直接当成可覆盖的安装目标。构建脚本必须先区分“用户真正执行的稳定入口”和“包管理器内部 shim/launcher”。

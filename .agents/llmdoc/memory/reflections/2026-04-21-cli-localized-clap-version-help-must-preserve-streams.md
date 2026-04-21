# CLI 本地化 clap 输出时必须保留 stdout/stderr 语义

## 背景
- 排查本分叉编译出的 `codex --version 2>/dev/null` 为空，而官方版本可以正常打印。
- 现场二进制实际会输出版本，但内容落在 stderr；例如 `/usr/local/bin/codex --version` 的 stderr 是 `codex-cli 0.222.0`，stdout 为 0 字节。

## 结论
- 根因不是版本号未注入，也不是 `workspace.package.version = "0.0.0"` 本身导致“空输出”。
- 真正回归点是 CLI 汉化后引入的自定义 clap 解析出口：`parse_multitool_cli()` 把 `err.to_string()` 一律写到 stderr，再 `exit(err.exit_code())`。
- clap 的 `DisplayHelp` / `DisplayVersion` 原本应走 stdout；把这两类也写到 stderr 会让安装脚本、shell 管道和 `2>/dev/null` 这类调用把版本误判为空。

## 证据
- 当前分叉 `codex-rs/cli/src/main.rs` 的 `parse_multitool_cli()` 使用 `try_get_matches_from()` 后手写错误输出。
- upstream `openai/main` 仍直接走 `MultitoolCli::parse()`，保留 clap 默认流行为。
- `scripts/install/install.sh` 的 `version_from_binary()` 明确从 stdout 解析版本：`"$codex_path" --version 2>/dev/null | sed ...`；因此 stderr 回归会直接让安装器拿不到版本。

## 处理
- 修复时不要按错误/非错误自己硬编码输出流，优先复用 clap 的 `err.use_stderr()` 语义。
- 至少补一个 CLI 集成测试，锁定顶层 `--version` 必须写到 stdout 且 stderr 为空，避免以后再次在本地化或自定义帮助渲染时回归。

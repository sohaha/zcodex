# npm 发布

使用仓库根目录的暂存脚本生成发布用的 npm tarball。例如，以下命令会为 `0.6.0` 版本暂存 CLI、responses 代理与 SDK 包：

```bash
./scripts/stage_npm_packages.py \
  --release-version 0.6.0 \
  --package codex \
  --package codex-responses-api-proxy \
  --package codex-sdk
```

脚本会一次性下载原生制品，为每个包填充 `vendor/`，并将 tarball 输出到 `dist/npm/`。

当传入 `--package codex` 时，暂存脚本会构建轻量的 `@sohaha/zcodex` 元包，以及后续会按平台 dist-tag 发布的各平台原生 `@sohaha/zcodex` 变体。

如果需要直接调用 `build_npm_package.py`，请先运行 `codex-cli/scripts/install_native_deps.py`，并传入指向已填充 `vendor/` 目录的 `--vendor-src`。

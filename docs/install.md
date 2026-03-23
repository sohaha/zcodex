## Installing & building

### System requirements

| Requirement                 | Details                                                         |
| --------------------------- | --------------------------------------------------------------- |
| Operating systems           | macOS 12+, Ubuntu 20.04+/Debian 10+, or Windows 11 **via WSL2** |
| Git (optional, recommended) | 2.23+ for built-in PR helpers                                   |
| RAM                         | 4-GB minimum (8-GB recommended)                                 |

### DotSlash

The GitHub Release also contains a [DotSlash](https://dotslash-cli.com/) file for the Codex CLI named `codex`. Using a DotSlash file makes it possible to make a lightweight commit to source control to ensure all contributors use the same version of an executable, regardless of what platform they use for development.

### Build from source

```bash
# Clone the repository and navigate to the root of the Cargo workspace.
git clone https://github.com/sohaha/zcodex.git
cd zcodex/codex-rs

# Install the Rust toolchain, if necessary.
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"
rustup component add rustfmt
rustup component add clippy
# Install helper tools used by the workspace justfile:
cargo install just
# Optional: install nextest for the `just test` helper
cargo install --locked cargo-nextest

# Build Codex.
cargo build

# Launch the TUI with a sample prompt.
cargo run --bin codex -- "explain this codebase to me"

# After making changes, use the root justfile helpers (they default to codex-rs):
just fmt
just fix -p <crate-you-touched>

# Run the relevant tests (project-specific is fastest), for example:
cargo test -p codex-tui
# If you have cargo-nextest installed, `just test` runs the test suite via nextest:
just test
# Avoid `--all-features` for routine local runs because it increases build
# time and `target/` disk usage by compiling additional feature combinations.
# If you specifically want full feature coverage, use:
cargo test --all-features
```

### Common `mise` tasks

If you use `mise`, the repository also provides a few shortcut tasks around the
Rust workspace:

```bash
# Show task help
mise run build help
mise run deps help

# Install host build dependencies
mise run deps
# equivalent explicit form:
mise run deps host

# Build the local Codex CLI
mise run build
# equivalent aliases:
mise run build cli
mise run build codex

# Run crate tests through the shared wrapper
mise run test core
mise run test tui
mise run test app-server-protocol

# You can still pass through cargo test selectors/filters:
mise run test core remote_models_request_times_out_after_5s
mise run test codex-core read_file_tools_run_in_parallel

# Preview (dry-run) and clear completed GitHub Actions runs for sohaha/zcodex.
# Requires a token with Actions write permission.
GITHUB_TOKEN=<token> mise run clear-actions
GITHUB_TOKEN=<token> mise run clear-actions -- --yes
```

`clear-actions` defaults to dry-run. Use `-- --yes` to pass the confirmation
flag through `mise run` and execute deletion.

For `codex-core`, `codex-cli`, and `codex-tui`, the shared `mise run test`
wrapper automatically prebuilds the workspace binaries that those tests expect
to find locally on Linux.

### Ubuntu cross-build to macOS arm64

If you use `mise`, the repository also provides dedicated tasks for building the
CLI from Ubuntu for Apple Silicon (`aarch64-apple-darwin`):

```bash
# Install Zig/cargo-zigbuild/Rust target and automatically fetch a default
# public macOS SDK into `.cache/macos-sdk`. The task now downloads/extracts
# Zig directly so it is not limited by mise's default HTTP timeout.
mise run deps ubuntu-macos-arm64

# Optional overrides if you want to pin a specific SDK source:
MACOS_SDK_URL=https://.../MacOSX.sdk.tar.xz mise run deps ubuntu-macos-arm64
MACOS_SDK_PATH=/path/to/MacOSX.sdk mise run deps ubuntu-macos-arm64
MACOS_SDK_TARBALL=/path/to/MacOSX.sdk.tar.xz mise run deps ubuntu-macos-arm64

# Optional Zig overrides for mirrors, local archives, or preinstalled binaries:
ZIG_BASE_URL=https://mirror.example.com/zig mise run deps ubuntu-macos-arm64
ZIG_URL=https://mirror.example.com/zig/0.14.0/zig-linux-x86_64-0.14.0.tar.xz mise run deps ubuntu-macos-arm64
ZIG_TARBALL=/path/to/zig-linux-x86_64-0.14.0.tar.xz mise run deps ubuntu-macos-arm64
ZIG_PATH=/path/to/zig mise run deps ubuntu-macos-arm64

# Build codex for macOS arm64 from Ubuntu.
mise run build ubuntu-macos-arm64 --release
```

构建完成后，任务除了打印二进制路径外，还会额外提示你在 macOS 上运行：

```bash
codesign --force --sign - /path/to/codex
```

By default, the dependency task downloads a public prepackaged macOS SDK. If
you need stricter provenance or a pinned SDK version, override the source with
`MACOS_SDK_URL`, `MACOS_SDK_TARBALL`, or `MACOS_SDK_PATH`.

For slow or filtered networks, you can also point Zig downloads at a mirror or
local artifact with `ZIG_BASE_URL`, `ZIG_URL`, `ZIG_TARBALL`, or `ZIG_PATH`.
If you need to tune download behavior further, the tasks also honor
`DOWNLOAD_CONNECT_TIMEOUT`, `DOWNLOAD_RETRY_COUNT`, `DOWNLOAD_RETRY_DELAY`,
`DOWNLOAD_MAX_TIME`, and `DOWNLOAD_METADATA_MAX_TIME`.

Ubuntu 交叉编译任务产出的是 `release` 二进制，但它**不会**执行 Apple
签名或公证。如果你把这个二进制直接拷到 Mac 上运行，macOS 可能会拦截或
直接杀掉进程，直到你在本地重新签名，或者走正式发布流程。

### macOS 签名与公证

如果你要分发 macOS 产物，至少需要准备两样东西：

1. 导出成 `.p12` 的 `Developer ID Application` 证书
2. 给 `notarytool` 使用的 App Store Connect API Key
   也就是 `.p8`、`Key ID`、`Issuer ID`

仓库里现在提供了两个辅助脚本：

```bash
# 把发布 workflow 需要的 5 个 GitHub Actions secrets 上传到仓库
./scripts/setup_macos_signing_secrets.sh --help

# 在 macOS 本地对二进制、.app、.dmg、.pkg 做签名和公证
./scripts/macos_sign_and_notarize_local.sh --help
```

#### 生成签名证书（`.p12`）

在一台 Mac 上操作：

1. 打开“钥匙串访问”（`Keychain Access`）
2. 在顶部菜单栏选择“钥匙串访问” > “证书助理” > “从证书颁发机构请求证书…”
3. 填写邮箱和常用名称，选择“存储到磁盘”，生成并保存 `CSR` 文件
4. 登录 Apple Developer，进入 `Certificates, Identifiers & Profiles`
5. 在 “Certificates” 页面点击右上角的 `+`
6. 选择 `Developer ID Application` 证书类型，并上传刚才生成的 `CSR` 文件
7. 证书生成后，下载得到的 `.cer` 文件，并双击导入“钥匙串访问”
8. 在“我的证书”（`My Certificates`）中找到该证书，确认它下面带有对应私钥
9. 选中这张证书，右键选择“导出”，导出为 `.p12` 文件
10. 导出时设置一个密码，这个密码就是后续要用的 `APPLE_CERTIFICATE_PASSWORD`

如果导入后的证书下面看不到私钥，说明 CSR 不是在当前这台 Mac 或当前
keychain 中生成的，这种情况下无法从这台机器导出可用的 `.p12`。

Apple 参考文档：

- https://developer.apple.com/help/account/certificates/create-a-certificate-signing-request
- https://developer.apple.com/help/account/certificates/create-developer-id-certificates
- https://support.apple.com/guide/keychain-access/import-and-export-keychain-items-kyca35961/mac

#### 生成公证用 API Key（`.p8`）

在 App Store Connect 中操作：

1. 打开 `Users and Access > Integrations > Team Keys`
2. 如果组织还没启用 API 访问，先开通
3. 新建一个 API Key
4. 下载 `.p8` 文件
5. 记下 `Key ID` 和 `Issuer ID`

注意：`.p8` 只能下载一次，丢失后需要重新生成。

Apple 参考文档：

- https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api

#### 配置 GitHub Actions secrets

当前发布 workflow 需要以下仓库 secrets：

- `APPLE_CERTIFICATE_P12`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_NOTARIZATION_KEY_P8`
- `APPLE_NOTARIZATION_KEY_ID`
- `APPLE_NOTARIZATION_ISSUER_ID`

可以直接用辅助脚本从本地文件上传：

```bash
gh auth login

APPLE_CERTIFICATE_PASSWORD='你的 p12 密码' \
./scripts/setup_macos_signing_secrets.sh \
  --p12 /绝对路径/DeveloperIDApplication.p12 \
  --p8 /绝对路径/AuthKey_ABC123XYZ.p8 \
  --key-id ABC123XYZ \
  --issuer-id 00000000-0000-0000-0000-000000000000 \
  --repo owner/repo
```

如果你想先校验参数而不真正上传，可以先加 `--dry-run`。

#### 在 macOS 本地签名和公证

先查看当前 keychain 里可用的签名身份：

```bash
security find-identity -v -p codesigning
```

然后对单个二进制做签名和公证：

```bash
./scripts/macos_sign_and_notarize_local.sh \
  --identity "Developer ID Application: Example, Inc. (TEAMID)" \
  --binary ./codex \
  --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
  --key-id ABC123XYZ \
  --issuer-id 00000000-0000-0000-0000-000000000000
```

或者对 `.app` 和 `.dmg` 一起处理：

```bash
./scripts/macos_sign_and_notarize_local.sh \
  --identity "Developer ID Application: Example, Inc. (TEAMID)" \
  --app ./Codex.app \
  --dmg ./Codex.dmg \
  --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
  --key-id ABC123XYZ \
  --issuer-id 00000000-0000-0000-0000-000000000000
```

如果要处理 `.pkg`，需要额外提供 Installer 证书身份：

```bash
./scripts/macos_sign_and_notarize_local.sh \
  --identity "Developer ID Application: Example, Inc. (TEAMID)" \
  --installer-identity "Developer ID Installer: Example, Inc. (TEAMID)" \
  --pkg ./Codex.pkg \
  --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
  --key-id ABC123XYZ \
  --issuer-id 00000000-0000-0000-0000-000000000000
```

这个脚本会：

- 对 `.pkg` 执行 `productsign --timestamp`，对其他目标执行 `codesign --timestamp`
- 在适用的目标上附加 `--options runtime`
- 用 `xcrun notarytool submit --wait` 提交公证
- 在公证通过后对 `.app`、`.dmg`、`.pkg` 执行 `staple`

对于单独的 Mach-O 二进制，脚本会先临时打成 zip 再提交公证。签名后会做本地
校验，但它不像 `.app` 或 `.dmg` 那样走同一套 `staple` 流程。

Apple 公证参考文档：

- https://developer.apple.com/documentation/security/customizing-the-notarization-workflow

如果你只是想在自己的 Mac 上本地测试，一个 ad-hoc 签名通常就够了：

```bash
codesign --force --sign - ./codex
./codex --help
```

### Ubuntu cross-build to Windows

If you use `mise`, the repository also provides tasks for building the CLI from
Ubuntu for Windows:

```bash
# Install Zig/cargo-zigbuild and the Rust Windows targets.
# amd64 -> x86_64-pc-windows-gnu
# arm64 -> aarch64-pc-windows-gnullvm
mise run deps ubuntu-win-amd64
mise run deps ubuntu-win-arm64

# Optional Zig overrides for mirrors, local archives, or preinstalled binaries:
# apply to either deps task:
ZIG_BASE_URL=https://mirror.example.com/zig mise run deps ubuntu-win-amd64
ZIG_URL=https://mirror.example.com/zig/0.14.0/zig-linux-x86_64-0.14.0.tar.xz mise run deps ubuntu-win-amd64
ZIG_TARBALL=/path/to/zig-linux-x86_64-0.14.0.tar.xz mise run deps ubuntu-win-amd64
ZIG_PATH=/path/to/zig mise run deps ubuntu-win-amd64

# Build codex.exe for Windows amd64 or arm64 from Ubuntu.
mise run build ubuntu-win-amd64 --release
mise run build ubuntu-win-arm64 --release

# Optionally package the local cross-built binaries into zip archives.
mise run package ubuntu-win
# Or call the script directly if you want to choose a custom output dir.
./scripts/package_windows_cross_builds.sh
```

The arm64 task uses Rust target `aarch64-pc-windows-gnullvm`, because the
current Linux Rust toolchain does not expose `aarch64-pc-windows-gnu`.

## Tracing / verbose logging

Codex is written in Rust, so it honors the `RUST_LOG` environment variable to configure its logging behavior.

The TUI defaults to `RUST_LOG=codex_core=info,codex_tui=info,codex_rmcp_client=info` and log messages are written to `~/.codex/log/codex-tui.log` by default. For a single run, you can override the log directory with `-c log_dir=...` (for example, `-c log_dir=./.codex-log`).

```bash
tail -F ~/.codex/log/codex-tui.log
```

By comparison, the non-interactive mode (`codex exec`) defaults to `RUST_LOG=error`, but messages are printed inline, so there is no need to monitor a separate file.

See the Rust documentation on [`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for more information on the configuration options.

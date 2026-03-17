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
git clone https://github.com/openai/codex.git
cd codex/codex-rs

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
```

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

By default, the dependency task downloads a public prepackaged macOS SDK. If
you need stricter provenance or a pinned SDK version, override the source with
`MACOS_SDK_URL`, `MACOS_SDK_TARBALL`, or `MACOS_SDK_PATH`.

For slow or filtered networks, you can also point Zig downloads at a mirror or
local artifact with `ZIG_BASE_URL`, `ZIG_URL`, `ZIG_TARBALL`, or `ZIG_PATH`.
If you need to tune download behavior further, the tasks also honor
`DOWNLOAD_CONNECT_TIMEOUT`, `DOWNLOAD_RETRY_COUNT`, `DOWNLOAD_RETRY_DELAY`,
`DOWNLOAD_MAX_TIME`, and `DOWNLOAD_METADATA_MAX_TIME`.

The Ubuntu cross-build task produces a release binary, but it does **not**
perform Apple code signing or notarization. If you copy that binary to a Mac
and run it directly, macOS may block or kill it until you sign it locally or
ship it through the release workflow.

### macOS code signing and notarization

For distributable macOS artifacts, you need both:

1. A `Developer ID Application` certificate exported as `.p12`
2. An App Store Connect API key for `notarytool` (`.p8`, Key ID, Issuer ID)

The repository now includes two helper scripts:

```bash
# Upload the 5 GitHub Actions secrets expected by the release workflow.
./scripts/setup_macos_signing_secrets.sh --help

# Sign + notarize local binaries, .app bundles, .dmg, or .pkg on macOS.
./scripts/macos_sign_and_notarize_local.sh --help
```

#### Generate the signing certificate (`.p12`)

On a Mac:

1. Open `Keychain Access`
2. Choose `Keychain Access > Certificate Assistant > Request a Certificate from a Certificate Authority`
3. Save the CSR to disk
4. In Apple Developer, go to `Certificates, Identifiers & Profiles > Certificates > +`
5. Create a `Developer ID Application` certificate using that CSR
6. Download the generated `.cer` and import it into Keychain Access
7. In `My Certificates`, confirm the certificate has its private key attached
8. Export it as `.p12` and choose an export password

If the imported certificate does not show a private key, you created the CSR on
another machine or in another keychain and cannot export a usable `.p12` from
this Mac.

Helpful Apple docs:

- https://developer.apple.com/help/account/certificates/create-a-certificate-signing-request
- https://developer.apple.com/help/account/certificates/create-developer-id-certificates
- https://support.apple.com/guide/keychain-access/import-and-export-keychain-items-kyca35961/mac

#### Generate the notarization API key (`.p8`)

In App Store Connect:

1. Open `Users and Access > Integrations > Team Keys`
2. Enable API access if your organization has not done so already
3. Generate a new API key
4. Download the `.p8` file exactly once
5. Record the `Key ID` and `Issuer ID`

Apple doc:

- https://developer.apple.com/help/app-store-connect/get-started/app-store-connect-api

#### Configure GitHub Actions secrets

The release workflow expects these repository secrets:

- `APPLE_CERTIFICATE_P12`
- `APPLE_CERTIFICATE_PASSWORD`
- `APPLE_NOTARIZATION_KEY_P8`
- `APPLE_NOTARIZATION_KEY_ID`
- `APPLE_NOTARIZATION_ISSUER_ID`

Use the helper script to upload them from local files:

```bash
gh auth login

APPLE_CERTIFICATE_PASSWORD='your-p12-password' \
./scripts/setup_macos_signing_secrets.sh \
  --p12 /absolute/path/DeveloperIDApplication.p12 \
  --p8 /absolute/path/AuthKey_ABC123XYZ.p8 \
  --key-id ABC123XYZ \
  --issuer-id 00000000-0000-0000-0000-000000000000 \
  --repo owner/repo
```

Use `--dry-run` first if you want input validation without uploading anything.

#### Sign and notarize locally on macOS

First, list the signing identities available in your keychain:

```bash
security find-identity -v -p codesigning
```

Then sign and notarize a standalone binary:

```bash
./scripts/macos_sign_and_notarize_local.sh \
  --identity "Developer ID Application: Example, Inc. (TEAMID)" \
  --binary ./codex \
  --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
  --key-id ABC123XYZ \
  --issuer-id 00000000-0000-0000-0000-000000000000
```

Or sign and notarize an app bundle plus dmg:

```bash
./scripts/macos_sign_and_notarize_local.sh \
  --identity "Developer ID Application: Example, Inc. (TEAMID)" \
  --app ./Codex.app \
  --dmg ./Codex.dmg \
  --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
  --key-id ABC123XYZ \
  --issuer-id 00000000-0000-0000-0000-000000000000
```

The helper script:

- signs every target with `codesign --timestamp`
- adds `--options runtime` where appropriate
- submits binaries through `xcrun notarytool submit --wait`
- staples `.app`, `.dmg`, and `.pkg` targets after notarization

For standalone Mach-O binaries, notarization is submitted with a temporary zip.
Those binaries are verified locally after signing, but they do not support the
same stapling flow as `.app` or `.dmg`.

Apple notarization docs:

- https://developer.apple.com/documentation/security/customizing-the-notarization-workflow

If you only need a local test binary on your own Mac, an ad-hoc signature is
often enough:

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

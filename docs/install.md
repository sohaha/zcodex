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

### Ubuntu cross-build to macOS arm64

If you use `mise`, the repository also provides dedicated tasks for building the
CLI from Ubuntu for Apple Silicon (`aarch64-apple-darwin`):

```bash
# Install Zig/cargo-zigbuild/Rust target and automatically fetch a default
# public macOS SDK into `.cache/macos-sdk`. The task now downloads/extracts
# Zig directly so it is not limited by mise's default HTTP timeout.
mise run deps-ubuntu-macos-arm64

# Optional overrides if you want to pin a specific SDK source:
MACOS_SDK_URL=https://.../MacOSX.sdk.tar.xz mise run deps-ubuntu-macos-arm64
MACOS_SDK_PATH=/path/to/MacOSX.sdk mise run deps-ubuntu-macos-arm64
MACOS_SDK_TARBALL=/path/to/MacOSX.sdk.tar.xz mise run deps-ubuntu-macos-arm64

# Optional Zig overrides for mirrors, local archives, or preinstalled binaries:
ZIG_BASE_URL=https://mirror.example.com/zig mise run deps-ubuntu-macos-arm64
ZIG_URL=https://mirror.example.com/zig/0.14.0/zig-linux-x86_64-0.14.0.tar.xz mise run deps-ubuntu-macos-arm64
ZIG_TARBALL=/path/to/zig-linux-x86_64-0.14.0.tar.xz mise run deps-ubuntu-macos-arm64
ZIG_PATH=/path/to/zig mise run deps-ubuntu-macos-arm64

# Build codex for macOS arm64 from Ubuntu.
mise run build-ubuntu-macos-arm64 --release
```

By default, the dependency task downloads a public prepackaged macOS SDK. If
you need stricter provenance or a pinned SDK version, override the source with
`MACOS_SDK_URL`, `MACOS_SDK_TARBALL`, or `MACOS_SDK_PATH`.

For slow or filtered networks, you can also point Zig downloads at a mirror or
local artifact with `ZIG_BASE_URL`, `ZIG_URL`, `ZIG_TARBALL`, or `ZIG_PATH`.
If you need to tune download behavior further, the tasks also honor
`DOWNLOAD_CONNECT_TIMEOUT`, `DOWNLOAD_RETRY_COUNT`, `DOWNLOAD_RETRY_DELAY`,
`DOWNLOAD_MAX_TIME`, and `DOWNLOAD_METADATA_MAX_TIME`.

## Tracing / verbose logging

Codex is written in Rust, so it honors the `RUST_LOG` environment variable to configure its logging behavior.

The TUI defaults to `RUST_LOG=codex_core=info,codex_tui=info,codex_rmcp_client=info` and log messages are written to `~/.codex/log/codex-tui.log` by default. For a single run, you can override the log directory with `-c log_dir=...` (for example, `-c log_dir=./.codex-log`).

```bash
tail -F ~/.codex/log/codex-tui.log
```

By comparison, the non-interactive mode (`codex exec`) defaults to `RUST_LOG=error`, but messages are printed inline, so there is no need to monitor a separate file.

See the Rust documentation on [`RUST_LOG`](https://docs.rs/env_logger/latest/env_logger/#enabling-logging) for more information on the configuration options.

set working-directory := "codex-rs"
set positional-arguments

target-dir slot:
    bash ./scripts/resolve-cargo-target-dir.sh "{{slot}}"

cargo-home slot:
    bash ./scripts/resolve-cargo-home-dir.sh "{{slot}}"

# Display help
help:
    just -l

# `codex`
alias c := codex
codex *args:
    export CARGO_TARGET_DIR="$(just target-dir run-codex)"; \
    cargo run --bin codex -- "$@"

# `codex exec`
exec *args:
    export CARGO_TARGET_DIR="$(just target-dir run-codex-exec)"; \
    cargo run --bin codex -- exec "$@"

# Run the CLI version of the file-search crate.
file-search *args:
    export CARGO_TARGET_DIR="$(just target-dir run-file-search)"; \
    cargo run --bin codex-file-search -- "$@"

# Build the CLI and run the app-server test client
app-server-test-client *args:
    export CARGO_TARGET_DIR="$(just target-dir run-app-server-test-client)"; \
    cargo build -p codex-cli; \
    cargo run -p codex-app-server-test-client -- --codex-bin "$CARGO_TARGET_DIR/debug/codex" "$@"

# format code
fmt:
    cargo fmt -- --config imports_granularity=Item 2>/dev/null

# 为不同 cargo 流程分配独立 target 子目录，减少多会话并发时的 build lock。
# 如需把同一条命令再拆到独立 lane，可额外设置 `CODEX_CARGO_LANE=<name>`。
fix *args:
    export CARGO_HOME="$(just cargo-home fix)"; \
    export CARGO_TARGET_DIR="$(just target-dir fix)"; \
    cargo clippy --fix --tests --allow-dirty "$@"

clippy *args:
    export CARGO_HOME="$(just cargo-home clippy)"; \
    export CARGO_TARGET_DIR="$(just target-dir clippy)"; \
    cargo clippy --tests "$@"

install:
    rustup show active-toolchain
    mise run dev-tools
    cargo fetch

# Run `cargo nextest` since it's faster than `cargo test`, though including
# --no-fail-fast is important to ensure all tests are run.
#
# Run `cargo install cargo-nextest` if you don't have it installed.
# Prefer this for routine local runs. Workspace crate features are banned, so
# there should be no need to add `--all-features`.
test:
    if command -v sccache >/dev/null 2>&1; then \
      export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"; \
      export SCCACHE_DIR="${SCCACHE_DIR:-/workspace/.cache/sccache}"; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    export CARGO_HOME="$(just cargo-home nextest-workspace)"; \
    export CARGO_TARGET_DIR="$(just target-dir nextest-workspace)"; \
    cargo nextest run --no-fail-fast

[no-cd]
build-test:
    mise run test build

# Fast local loop for codex-core. Uses more disk for build caches to reduce
# repeated compile time.
core-test-fast *args:
    if command -v sccache >/dev/null 2>&1; then \
      export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"; \
      export SCCACHE_DIR="${SCCACHE_DIR:-/workspace/.cache/sccache}"; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    export CARGO_HOME="$(just cargo-home nextest-core)"; \
    export CARGO_TARGET_DIR="$(just target-dir nextest-core)"; \
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo nextest run -p codex-core --no-fail-fast --test all "$@"; \
    else \
      cargo test -p codex-core --test all "$@"; \
    fi

# Fast local loop for codex-app-server. Uses more disk for build caches to
# reduce repeated compile time.
app-server-test-fast *args:
    if command -v sccache >/dev/null 2>&1; then \
      export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"; \
      export SCCACHE_DIR="${SCCACHE_DIR:-/workspace/.cache/sccache}"; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    export CARGO_HOME="$(just cargo-home nextest-app-server)"; \
    export CARGO_TARGET_DIR="$(just target-dir nextest-app-server)"; \
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo nextest run -p codex-app-server --no-fail-fast --test all "$@"; \
    else \
      cargo test -p codex-app-server --test all "$@"; \
    fi

# Fast local loop for codex-mcp-server. Uses its own cargo cache/target slot to
# reduce contention with other test runs.
mcp-server-test-fast *args:
    if command -v sccache >/dev/null 2>&1; then \
      export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"; \
      export SCCACHE_DIR="${SCCACHE_DIR:-/workspace/.cache/sccache}"; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    export CARGO_HOME="$(just cargo-home nextest-mcp-server)"; \
    export CARGO_TARGET_DIR="$(just target-dir nextest-mcp-server)"; \
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo nextest run -p codex-mcp-server --features tldr --no-fail-fast --test all "$@"; \
    else \
      cargo test -p codex-mcp-server --features tldr --test all "$@"; \
    fi

# Fast local loop for codex-mcp-server tests with the `tldr` feature enabled.
mcp-server-tldr-test-fast *args:
    just mcp-server-test-fast "$@"

# Fast local loop for the full tldr chain: native-tldr first, then
# codex-mcp-server with `--features tldr`, each using its own cache slot.
tldr-test-fast *args:
    just native-tldr-test-fast "$@"
    just mcp-server-tldr-test-fast "$@"

# Focused loop for daemon/status lifecycle coverage across native-tldr and the
# MCP bridge. Runs the smaller daemon-oriented subsets sequentially.
tldr-daemon-test-fast *args:
    just native-tldr-test-fast daemon "$@"
    just mcp-server-tldr-test-fast ping "$@"
    just mcp-server-tldr-test-fast warm "$@"
    just mcp-server-tldr-test-fast snapshot "$@"
    just mcp-server-tldr-test-fast status "$@"
    just mcp-server-tldr-test-fast notify "$@"

# Focused loop for semantic indexing/query coverage across native-tldr and the
# MCP bridge.
tldr-semantic-test-fast *args:
    just native-tldr-test-fast semantic "$@"
    just mcp-server-tldr-test-fast semantic "$@"

# Fast local loop for codex-mcp-server tests without extra features. Useful when
# tldr coverage is not needed and you want to avoid building that feature set.
mcp-server-core-test-fast *args:
    if command -v sccache >/dev/null 2>&1; then \
      export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"; \
      export SCCACHE_DIR="${SCCACHE_DIR:-/workspace/.cache/sccache}"; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    export CARGO_HOME="$(just cargo-home nextest-mcp-server-core)"; \
    export CARGO_TARGET_DIR="$(just target-dir nextest-mcp-server-core)"; \
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo nextest run -p codex-mcp-server --no-fail-fast --test all "$@"; \
    else \
      cargo test -p codex-mcp-server --test all "$@"; \
    fi

# Fast local loop for codex-native-tldr. Uses more disk for build caches to
# reduce repeated compile time.
native-tldr-test-fast *args:
    if command -v sccache >/dev/null 2>&1; then \
      export RUSTC_WRAPPER="${RUSTC_WRAPPER:-sccache}"; \
      export SCCACHE_DIR="${SCCACHE_DIR:-/workspace/.cache/sccache}"; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    export CARGO_HOME="$(just cargo-home nextest-native-tldr)"; \
    export CARGO_TARGET_DIR="$(just target-dir nextest-native-tldr)"; \
    if cargo nextest --version >/dev/null 2>&1; then \
      cargo nextest run -p codex-native-tldr --no-fail-fast "$@"; \
    else \
      cargo test -p codex-native-tldr "$@"; \
    fi

# Cross-check the Windows build for the native-tldr/cli chain from Linux/macOS.
# Override CODEX_WINDOWS_TARGET if you need a different triple.
windows-cross-check *args:
    target="${CODEX_WINDOWS_TARGET:-x86_64-pc-windows-msvc}"; \
    if command -v sccache >/dev/null 2>&1; then \
      export RUSTC_WRAPPER="sccache"; \
      export SCCACHE_DIR="${SCCACHE_DIR:-/workspace/.cache/sccache}"; \
    else \
      unset RUSTC_WRAPPER; \
    fi; \
    export CARGO_INCREMENTAL=0; \
    export CARGO_HOME="$(just cargo-home check-windows)"; \
    export CARGO_TARGET_DIR="$(just target-dir check-windows)"; \
    case "$target" in \
      *-windows-gnu) command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1 || { echo "missing x86_64-w64-mingw32-gcc for $target"; exit 2; } ;; \
      *-windows-msvc) command -v lib.exe >/dev/null 2>&1 || { echo "missing lib.exe for $target"; exit 2; } ;; \
    esac; \
    rustup target list --installed | grep -qx "$target" || rustup target add "$target"; \
    cargo check -p codex-native-tldr -p codex-cli --target "$target" "$@"

# Build and run Codex from source using Bazel.
# Note we have to use the combination of `[no-cd]` and `--run_under="cd $PWD &&"`
# to ensure that Bazel runs the command in the current working directory.
[no-cd]
bazel-codex *args:
    bazel run //codex-rs/cli:codex --run_under="cd $PWD &&" -- "$@"

[no-cd]
bazel-lock-update:
    bazel mod deps --lockfile_mode=update

[no-cd]
bazel-lock-check:
    ./scripts/check-module-bazel-lock.sh

bazel-test:
    bazel test --test_tag_filters=-argument-comment-lint //... --keep_going

[no-cd]
bazel-clippy:
    bazel_targets="$(./scripts/list-bazel-clippy-targets.sh)" && bazel build --config=clippy -- ${bazel_targets}

[no-cd]
bazel-argument-comment-lint:
    bazel build --config=argument-comment-lint -- $(./tools/argument-comment-lint/list-bazel-targets.sh)
bazel-remote-test:
    bazel test --test_tag_filters=-argument-comment-lint //... --config=remote --platforms=//:rbe --keep_going

build-for-release:
    bazel build //codex-rs/cli:release_binaries --config=remote

# Run the MCP server
mcp-server-run *args:
    export CARGO_TARGET_DIR="$(just target-dir run-mcp-server)"; \
    cargo run -p codex-mcp-server -- "$@"

# Regenerate the json schema for config.toml from the current config types.
write-config-schema:
    export CARGO_TARGET_DIR="$(just target-dir run-write-config-schema)"; \
    cargo run -p codex-core --bin codex-write-config-schema

# Regenerate vendored app-server protocol schema artifacts.
write-app-server-schema *args:
    export CARGO_TARGET_DIR="$(just target-dir run-write-app-server-schema)"; \
    cargo run -p codex-app-server-protocol --bin write_schema_fixtures -- "$@"

[no-cd]
write-hooks-schema:
    export CARGO_TARGET_DIR="$(bash ./codex-rs/scripts/resolve-cargo-target-dir.sh run-write-hooks-schema)"; \
    cargo run --manifest-path ./codex-rs/Cargo.toml -p codex-hooks --bin write_hooks_schema_fixtures

# Run the argument-comment Dylint checks across codex-rs.
[no-cd]
argument-comment-lint *args:
    if [ "$#" -eq 0 ]; then \
      bazel build --config=argument-comment-lint -- $(./tools/argument-comment-lint/list-bazel-targets.sh); \
    else \
      ./tools/argument-comment-lint/run-prebuilt-linter.py "$@"; \
    fi

[no-cd]
argument-comment-lint-from-source *args:
    ./tools/argument-comment-lint/run.py "$@"
# Tail logs from the state SQLite database
log *args:
    export CARGO_TARGET_DIR="$(just target-dir run-log)"; \
    if [ "${1:-}" = "--" ]; then shift; fi; cargo run -p codex-state --bin logs_client -- "$@"

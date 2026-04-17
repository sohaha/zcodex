set working-directory := "codex-rs"
set positional-arguments

# Display help
help:
    just -l

# `codex`
alias c := codex
codex *args:
    cargo run --bin codex -- "$@"

# `codex exec`
exec *args:
    cargo run --bin codex -- exec "$@"

# Start `codex exec-server` and run codex-tui.
[no-cd]
tui-with-exec-server *args:
    ./scripts/run_tui_with_exec_server.sh "$@"

# Run the CLI version of the file-search crate.
file-search *args:
    cargo run --bin codex-file-search -- "$@"

# Build the CLI and run the app-server test client
app-server-test-client *args:
    cargo build -p codex-cli
    cargo run -p codex-app-server-test-client -- --codex-bin ./target/debug/codex "$@"

# format code
fmt:
    cargo fmt -- --config imports_granularity=Item 2>/dev/null

fix *args:
    cargo clippy --fix --tests --allow-dirty "$@"

clippy *args:
    cargo clippy --tests "$@"

install:
    rustup show active-toolchain
    cargo fetch

# Run `cargo nextest` since it's faster than `cargo test`, though including
# --no-fail-fast is important to ensure all tests are run.
#
# Run `cargo install cargo-nextest` if you don't have it installed.
# Prefer this for routine local runs. Workspace crate features are banned, so
# there should be no need to add `--all-features`.
test:
    cargo nextest run --no-fail-fast

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
    cargo run -p codex-mcp-server -- "$@"

# Regenerate the json schema for config.toml from the current config types.
write-config-schema:
    cargo run -p codex-core --bin codex-write-config-schema

# Regenerate vendored app-server protocol schema artifacts.
write-app-server-schema *args:
    cargo run -p codex-app-server-protocol --bin write_schema_fixtures -- "$@"

[no-cd]
write-hooks-schema:
    cargo run --manifest-path ./codex-rs/Cargo.toml -p codex-hooks --bin write_hooks_schema_fixtures

TARGET_WINDOWS_GNU := "x86_64-pc-windows-gnullvm"
TARGET_WINDOWS_MSVC := "x86_64-pc-windows-msvc"
WINDOWS_CROSS_PKGS := "codex-native-tldr codex-cli"

# Cross-compile check for Windows targets (native-tldr + cli).
# Requires mingw-w64 for GNU target, or MSVC SDK for MSVC target.
#
# Usage:
#   just windows-cross-check          # check both GNU and MSVC
#   just windows-cross-check gnu      # check GNU only
#   just windows-cross-check msvc     # check MSVC only
windows-cross-check *args:
    #!/usr/bin/env bash
    set -euo pipefail
    rustup target add {{ TARGET_WINDOWS_GNU }} {{ TARGET_WINDOWS_MSVC }} 2>/dev/null || true
    targets=()
    if [ -z "{{ args }}" ] || echo "{{ args }}" | grep -qw "gnu"; then
        targets+=("{{ TARGET_WINDOWS_GNU }}")
    fi
    if [ -z "{{ args }}" ] || echo "{{ args }}" | grep -qw "msvc"; then
        targets+=("{{ TARGET_WINDOWS_MSVC }}")
    fi
    failed=0
    for target in "${targets[@]}"; do
        for pkg in {{ WINDOWS_CROSS_PKGS }}; do
            echo "=== Checking ${pkg} (${target}) ==="
            if ! RUSTC_WRAPPER= CARGO_TARGET_DIR="target/${target}" cargo check -p "${pkg}" --target "${target}"; then
                echo "  ⚠ ${pkg} failed for ${target} (missing cross C toolchain?)" >&2
                failed=1
            else
                echo "  ✓ ${pkg} OK for ${target}"
            fi
        done
    done
    if [ "${failed}" -ne 0 ]; then
        echo ""
        echo "Some checks failed. Common fixes:"
        echo "  • GNU:  apt install mingw-w64"
        echo "  • MSVC: run on Windows or install MSVC SDK + linker"
        exit 1
    fi
    echo "All Windows cross-checks passed."

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
    if [ "${1:-}" = "--" ]; then shift; fi; cargo run -p codex-state --bin logs_client -- "$@"

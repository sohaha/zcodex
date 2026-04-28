#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$script_dir/../../.." rev-parse --show-toplevel)"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local message="$3"

  if [[ "$haystack" != *"$needle"* ]]; then
    echo "assertion failed: $message" >&2
    echo "  expected to find: $needle" >&2
    echo "  actual output: $haystack" >&2
    exit 1
  fi
}

run_tests() {
  local temp_root
  local task_dir
  local fake_bin_dir
  local sdk_root
  local output

  temp_root="$(mktemp -d)"
  trap "rm -rf '$temp_root'" EXIT
  task_dir="$temp_root/tasks"
  fake_bin_dir="$temp_root/bin"
  sdk_root="$temp_root/macos-sdk"

  mkdir -p "$task_dir" "$fake_bin_dir"
  cp "$repo_root/.mise/tasks/deps-ubuntu-macos-arm64" "$task_dir/deps-ubuntu-macos-arm64"
  cp "$repo_root/.mise/tasks/lib-speed-first-build" "$task_dir/lib-speed-first-build"
  cp "$repo_root/.mise/tasks/lib-ubuntu-macos-arm64" "$task_dir/lib-ubuntu-macos-arm64"

  cat >"$fake_bin_dir/apt-get" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
exit 0
EOF

  cat >"$fake_bin_dir/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "zigbuild" ] && [ "${2:-}" = "--help" ]; then
  exit 0
fi
exit 0
EOF

  cat >"$fake_bin_dir/rustup" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "target" ] && [ "${2:-}" = "list" ] && [ "${3:-}" = "--installed" ]; then
  printf 'aarch64-apple-darwin\n'
  exit 0
fi
if [ "${1:-}" = "show" ] && [ "${2:-}" = "active-toolchain" ]; then
  printf 'stable-x86_64-unknown-linux-gnu (default)\n'
  exit 0
fi
exit 0
EOF

  cat >"$fake_bin_dir/zig" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "version" ]; then
  printf '0.15.2\n'
fi
EOF

  cat >"$fake_bin_dir/curl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

output=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "-o" ]; then
    output="$arg"
    break
  fi
  previous="$arg"
done

if [ -n "$output" ]; then
  mkdir -p "$(dirname "$output")"
  printf 'fake sdk archive\n' >"$output"
  exit 0
fi

printf '[\n'
printf '  {"github_download_url": "https://example.invalid/MacOSX.sdk.tar.xz", "github_download_sha256sum": "fake-sha256"}'
perl -e 'print ",\n" . (" " x 200000) . "{\"ignored\": true}\n"'
printf ']\n'
EOF

  cat >"$fake_bin_dir/sha256sum" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
cat >/dev/null
exit 0
EOF

  cat >"$fake_bin_dir/tar" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

destination=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "-C" ]; then
    destination="$arg"
    break
  fi
  previous="$arg"
done

mkdir -p "$destination/MacOSX.sdk"
EOF

  chmod +x "$task_dir/deps-ubuntu-macos-arm64" \
    "$task_dir/lib-speed-first-build" \
    "$task_dir/lib-ubuntu-macos-arm64" \
    "$fake_bin_dir/apt-get" \
    "$fake_bin_dir/cargo" \
    "$fake_bin_dir/rustup" \
    "$fake_bin_dir/zig" \
    "$fake_bin_dir/curl" \
    "$fake_bin_dir/sha256sum" \
    "$fake_bin_dir/tar"

  output="$(
    PATH="$fake_bin_dir:$PATH" \
      CODEX_DISABLE_SCCACHE=1 \
      MACOS_SDK_ROOT="$sdk_root" \
      ZIG_PATH="$fake_bin_dir/zig" \
      bash "$task_dir/deps-ubuntu-macos-arm64" 2>&1
  )"

  assert_contains \
    "$output" \
    "[mise] macOS SDK 已就绪: $sdk_root/MacOSX.sdk" \
    "deps-ubuntu-macos-arm64 should parse default SDK metadata without SIGPIPE under pipefail"
}

run_tests

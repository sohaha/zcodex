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
  local sandbox_repo
  local task_dir
  local codex_rs_scripts_dir
  local fake_bin_dir
  local sdk_dir
  local ssh_hint_file
  local readonly_home
  local output_bin
  local rs_ext_args_file
  local cargo_lane_file
  local cc_search_dirs_file
  local host_cc_search_dirs_file
  local rs_ext_args
  local cargo_lane
  local cc_search_dirs
  local host_cc_search_dirs
  local output
  temp_root="$(mktemp -d)"
  trap "rm -rf '$temp_root'" EXIT
  sandbox_repo="$temp_root/repo"
  task_dir="$sandbox_repo/.mise/tasks"
  codex_rs_scripts_dir="$sandbox_repo/codex-rs/scripts"
  fake_bin_dir="$temp_root/bin"
  sdk_dir="$sandbox_repo/.cache/macos-sdk/MacOSX.sdk"
  ssh_hint_file="$temp_root/nm"
  readonly_home="$temp_root/readonly-home"
  output_bin="$sandbox_repo/test-target/aarch64-apple-darwin/release/codex"
  rs_ext_args_file="$temp_root/rs-ext-args"
  cargo_lane_file="$temp_root/cargo-lane"
  cc_search_dirs_file="$temp_root/cc-search-dirs"
  host_cc_search_dirs_file="$temp_root/host-cc-search-dirs"

  mkdir -p "$task_dir" "$codex_rs_scripts_dir" "$fake_bin_dir" "$sdk_dir" "$readonly_home"
  chmod 555 "$readonly_home"
  git -C "$sandbox_repo" init >/dev/null 2>&1

  cp "$repo_root/.mise/tasks/build-ubuntu-macos-arm64" "$task_dir/build-ubuntu-macos-arm64"
  cp "$repo_root/.mise/tasks/lib-remote-ssh-target" "$task_dir/lib-remote-ssh-target"

  cat >"$task_dir/lib-artifact-size" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

move_artifact_to_dist() {
  printf '%s\n' "$1"
}

print_artifact_size() {
  :
}
EOF

  cat >"$task_dir/lib-cargo-workspace-version" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

resolve_codex_release_version() {
  printf '1.0.0\n'
}

strip_codex_release_version_args() {
  printf '%s\0' "$@"
}

export_codex_release_version_if_present() {
  :
}

activate_cargo_workspace_version_override() {
  :
}
EOF

  cat >"$task_dir/lib-speed-first-build" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

configure_speed_first_build_defaults() {
  :
}

print_speed_first_build_summary() {
  :
}
EOF

  cat >"$task_dir/lib-ubuntu-macos-arm64" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

has_usable_rust_target() {
  return 0
}

ensure_zig_binary() {
  printf '%s\n' "$TEST_FAKE_ZIG"
}
EOF

  cat >"$task_dir/rs-ext" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

printf '%s\n' "$*" >"$TEST_RS_EXT_ARGS_FILE"
printf '%s\n' "${CODEX_CARGO_LANE:-}" >"$TEST_CARGO_LANE_FILE"
TARGET=aarch64-apple-darwin cc --print-search-dirs >"$TEST_CC_SEARCH_DIRS_FILE"
TARGET=x86_64-unknown-linux-gnu cc --print-search-dirs >"$TEST_HOST_CC_SEARCH_DIRS_FILE"
mkdir -p "$(dirname "$TEST_OUTPUT_BIN")"
perl - "$TEST_OUTPUT_BIN" <<'PL'
use strict;
use warnings;

my $path = shift @ARGV;
open my $fh, '>:raw', $path or die "open $path: $!";
print {$fh} pack('V8', 0xfeedfacf, 0, 0, 0, 1, 72, 0, 0);
print {$fh} pack(
  'VVa16QQQQiiVV',
  0x19,
  72,
  "__DATA_CONST\0\0\0\0",
  0,
  0,
  0,
  0,
  0,
  0,
  0,
  0,
);
close $fh or die "close $path: $!";
PL
chmod +x "$TEST_OUTPUT_BIN"
EOF

  cat >"$codex_rs_scripts_dir/resolve-cargo-slot.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'slot-macos-arm64\n'
EOF

  cat >"$codex_rs_scripts_dir/resolve-cargo-target-dir.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s/test-target\n' "$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
EOF

  cat >"$fake_bin_dir/cargo" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
exit 0
EOF

  cat >"$fake_bin_dir/rustup" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
exit 0
EOF

  cat >"$fake_bin_dir/clang" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "--print-search-dirs" ]; then
  printf 'programs: =/host-tools\n'
  printf 'libraries: =/host-libs\n'
fi
exit 0
EOF

  cat >"$fake_bin_dir/clang++" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "--print-search-dirs" ]; then
  printf 'programs: =/host-tools-cxx\n'
  printf 'libraries: =/host-libs-cxx\n'
fi
exit 0
EOF

  cat >"$fake_bin_dir/zig" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

cache_root="${ZIG_GLOBAL_CACHE_DIR:-$HOME/.cache/zig}"
mkdir -p "$cache_root/o/fake"
: >"$cache_root/o/fake/libubsan_rt.a"
: >"$cache_root/o/fake/libcompiler_rt.a"

out_path=""
previous=""
for arg in "$@"; do
  if [ "$previous" = "-o" ]; then
    out_path="$arg"
    break
  fi
  previous="$arg"
done

if [ -n "$out_path" ]; then
  mkdir -p "$(dirname "$out_path")"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$out_path"
  chmod +x "$out_path"
fi
EOF

  chmod +x \
    "$task_dir/build-ubuntu-macos-arm64" \
    "$task_dir/lib-artifact-size" \
    "$task_dir/lib-cargo-workspace-version" \
    "$task_dir/lib-remote-ssh-target" \
    "$task_dir/lib-speed-first-build" \
    "$task_dir/lib-ubuntu-macos-arm64" \
    "$task_dir/rs-ext" \
    "$codex_rs_scripts_dir/resolve-cargo-slot.sh" \
    "$codex_rs_scripts_dir/resolve-cargo-target-dir.sh" \
    "$fake_bin_dir/cargo" \
    "$fake_bin_dir/rustup" \
    "$fake_bin_dir/clang" \
    "$fake_bin_dir/clang++" \
    "$fake_bin_dir/zig"

  printf '%s\n' 'ssh user@example.com -p 22 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null' >"$ssh_hint_file"

  output="$(
    cd "$sandbox_repo"
    unset CNB_VSCODE_REMOTE_SSH_SCHEMA
    PATH="$fake_bin_dir:$PATH" \
      HOME="$readonly_home" \
      TEST_FAKE_ZIG="$fake_bin_dir/zig" \
      TEST_OUTPUT_BIN="$output_bin" \
      TEST_RS_EXT_ARGS_FILE="$rs_ext_args_file" \
      TEST_CARGO_LANE_FILE="$cargo_lane_file" \
      TEST_CC_SEARCH_DIRS_FILE="$cc_search_dirs_file" \
      TEST_HOST_CC_SEARCH_DIRS_FILE="$host_cc_search_dirs_file" \
      CNB_REMOTE_SSH_HINT_FILE="$ssh_hint_file" \
      bash "$task_dir/build-ubuntu-macos-arm64" 2>&1
  )"

  rs_ext_args="$(cat "$rs_ext_args_file")"
  cargo_lane="$(cat "$cargo_lane_file")"
  cc_search_dirs="$(cat "$cc_search_dirs_file")"
  host_cc_search_dirs="$(cat "$host_cc_search_dirs_file")"

  assert_contains \
    "$output" \
    "未检测到 CNB_VSCODE_REMOTE_SSH_SCHEMA，已改用 $ssh_hint_file 解析 SSH 目标" \
    "build-ubuntu-macos-arm64 should log when it falls back to the SSH hint file"

  assert_contains \
    "$output" \
    "scp user@example.com:$output_bin ~/.local/bin/codex" \
    "build-ubuntu-macos-arm64 should print the download command derived from the SSH hint file"

  assert_contains \
    "$rs_ext_args" \
    "--features codex-tui/realtime-webrtc-stub" \
    "build-ubuntu-macos-arm64 should enable the realtime stub on codex-tui instead of codex-cli"

  assert_contains \
    "$cargo_lane" \
    "macos-arm64-" \
    "build-ubuntu-macos-arm64 should isolate each default Cargo target lane to avoid stale artifact locks"

  assert_contains \
    "$cc_search_dirs" \
    "libraries: =$sandbox_repo/.cache/ubuntu-macos-arm64-toolchain/compiler-rt" \
    "build-ubuntu-macos-arm64 should expose a target-aware compiler runtime search root via cc --print-search-dirs"

  if [[ "$cc_search_dirs" == *"x86_64-linux-gnu"* ]]; then
    echo "assertion failed: build-ubuntu-macos-arm64 should not expose host linux linker paths via cc --print-search-dirs" >&2
    echo "  actual output: $cc_search_dirs" >&2
    exit 1
  fi

  assert_contains \
    "$host_cc_search_dirs" \
    "libraries: =/host-libs" \
    "build-ubuntu-macos-arm64 should fall back to the host compiler for host-side cc --print-search-dirs"

  if [[ "$host_cc_search_dirs" == *"$sandbox_repo/.cache/ubuntu-macos-arm64-toolchain/compiler-rt"* ]]; then
    echo "assertion failed: build-ubuntu-macos-arm64 should not leak target linker search dirs into host builds" >&2
    echo "  actual output: $host_cc_search_dirs" >&2
    exit 1
  fi

  if [ ! -e "$sandbox_repo/.cache/ubuntu-macos-arm64-toolchain/compiler-rt/lib/darwin/libclang_rt.osx.a" ]; then
    echo "assertion failed: build-ubuntu-macos-arm64 should materialize libclang_rt.osx.a in the synthetic darwin runtime search dir" >&2
    exit 1
  fi
}

run_tests

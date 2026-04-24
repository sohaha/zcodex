#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$script_dir/../../.." rev-parse --show-toplevel)"

source "$repo_root/.mise/tasks/build"

assert_eq() {
  local expected="$1"
  local actual="$2"
  local message="$3"

  if [ "$expected" != "$actual" ]; then
    echo "assertion failed: $message" >&2
    echo "  expected: $expected" >&2
    echo "  actual:   $actual" >&2
    exit 1
  fi
}

assert_file_contains() {
  local path="$1"
  local expected="$2"
  local message="$3"
  local actual

  actual="$(cat "$path")"
  assert_eq "$expected" "$actual" "$message"
}

run_tests() {
  local target_root
  local debug_slot
  local release_slot
  local cross_release_slot
  local built_codex_bin
  local installed_bin_dir
  local broken_target
  local home_dir
  local local_bin_dir
  local managed_node_bin_dir
  local managed_node_js_dir
  local system_path
  local original_path
  local original_home
  target_root="$(mktemp -d)"
  trap "rm -rf '$target_root'" EXIT
  debug_slot="$(bash "$repo_root/codex-rs/scripts/resolve-cargo-slot.sh" cargo build -p codex-cli --bin codex -j "$(nproc)")"
  release_slot="$(bash "$repo_root/codex-rs/scripts/resolve-cargo-slot.sh" cargo build -p codex-cli --bin codex -j "$(nproc)" --release)"
  cross_release_slot="$(bash "$repo_root/codex-rs/scripts/resolve-cargo-slot.sh" cargo build -p codex-cli --bin codex -j "$(nproc)" --release --target x86_64-unknown-linux-gnu)"
  built_codex_bin="$target_root/built-codex"
  installed_bin_dir="$target_root/bin"
  broken_target="$target_root/missing/codex"
  home_dir="$target_root/home"
  local_bin_dir="$home_dir/.local/bin"
  managed_node_bin_dir="$home_dir/.local/share/mise/installs/node/latest/bin"
  managed_node_js_dir="$home_dir/.local/share/mise/installs/node/25.9.0/lib/node_modules/@sohaha/zcodex/bin"
  system_path="/usr/bin:/bin"
  original_path="$PATH"
  original_home="${HOME:-}"

  mkdir -p "$installed_bin_dir"
  mkdir -p "$local_bin_dir"
  mkdir -p "$managed_node_bin_dir"
  mkdir -p "$managed_node_js_dir"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$built_codex_bin"
  chmod +x "$built_codex_bin"
  ln -s "$broken_target" "$installed_bin_dir/codex"
  printf '#!/usr/bin/env node\nconsole.log(\"shim\")\n' >"$managed_node_js_dir/codex.js"
  chmod +x "$managed_node_js_dir/codex.js"
  ln -s "$managed_node_js_dir/codex.js" "$managed_node_bin_dir/codex"

  CARGO_TARGET_DIR="$target_root" \
    assert_eq \
      "$target_root/$debug_slot/debug/codex" \
      "$(CARGO_TARGET_DIR="$target_root" resolve_codex_bin)" \
      "debug builds should resolve slot-scoped target by default"

  CARGO_TARGET_DIR="$target_root" \
    assert_eq \
      "$target_root/debug/codex" \
      "$(CARGO_TARGET_DIR="$target_root" resolve_active_codex_bin)" \
      "active target lookup should stay under active CARGO_TARGET_DIR"

  CARGO_TARGET_DIR="$target_root" \
    assert_eq \
      "$target_root/$release_slot/release/codex" \
      "$(CARGO_TARGET_DIR="$target_root" resolve_codex_bin --release)" \
      "release builds should resolve slot-scoped target by default"

  CARGO_TARGET_DIR="$target_root" \
    assert_eq \
      "$target_root/$cross_release_slot/x86_64-unknown-linux-gnu/release/codex" \
      "$(CARGO_TARGET_DIR="$target_root" resolve_codex_bin --release --target x86_64-unknown-linux-gnu)" \
      "cross-target release builds should stay under slot-scoped target dirs"

  CARGO_TARGET_DIR="$target_root" CODEX_CARGO_TARGET_DISABLE=1 \
    assert_eq \
      "$target_root/debug/codex" \
      "$(CARGO_TARGET_DIR="$target_root" CODEX_CARGO_TARGET_DISABLE=1 resolve_codex_bin)" \
      "disabling target isolation should resolve back to active CARGO_TARGET_DIR"

  export PATH="$installed_bin_dir:$original_path"
  assert_eq \
    "$installed_bin_dir/codex" \
    "$(find_installed_codex_path)" \
    "broken PATH symlink should still be discoverable for overwrite"

  overwrite_installed_codex_if_present "$built_codex_bin"

  if [ -L "$installed_bin_dir/codex" ]; then
    echo "assertion failed: broken PATH symlink should be replaced by a real binary" >&2
    exit 1
  fi
  if [ ! -x "$installed_bin_dir/codex" ]; then
    echo "assertion failed: fallback install target should stay executable" >&2
    exit 1
  fi
  if [ -e "$broken_target" ]; then
    echo "assertion failed: missing real target should not be recreated outside PATH" >&2
    exit 1
  fi
  assert_file_contains \
    "$installed_bin_dir/codex" \
    "$(cat "$built_codex_bin")" \
    "broken PATH symlink should be replaced with the built codex binary"

  export HOME="$home_dir"
  export PATH="$local_bin_dir:$system_path"

  overwrite_installed_codex_if_present "$built_codex_bin"

  if [ ! -x "$local_bin_dir/codex" ]; then
    echo "assertion failed: missing installed codex should be installed into a usable PATH dir" >&2
    exit 1
  fi
  assert_file_contains \
    "$local_bin_dir/codex" \
    "$(cat "$built_codex_bin")" \
    "missing installed codex should be installed into the preferred PATH dir"

  export PATH="$managed_node_bin_dir:$installed_bin_dir:$system_path"
  assert_eq \
    "$installed_bin_dir/codex" \
    "$(find_installed_codex_path)" \
    "managed node shim should be skipped when a real codex exists later on PATH"

  rm -f "$local_bin_dir/codex"
  export PATH="$managed_node_bin_dir:$system_path"
  assert_eq \
    "$local_bin_dir/codex" \
    "$(resolve_codex_install_target)" \
    "managed node shim should not become the overwrite target when no real codex exists"

  overwrite_installed_codex_if_present "$built_codex_bin"

  if [ ! -x "$local_bin_dir/codex" ]; then
    echo "assertion failed: managed node shim fallback should install into the normal user bin dir" >&2
    exit 1
  fi
  assert_file_contains \
    "$local_bin_dir/codex" \
    "$(cat "$built_codex_bin")" \
    "managed node shim fallback should install the built codex into the normal user bin dir"
  assert_file_contains \
    "$managed_node_js_dir/codex.js" \
    '#!/usr/bin/env node
console.log("shim")' \
    "managed node shim target should stay untouched"

  export PATH="$original_path"
  export HOME="$original_home"
}

run_tests

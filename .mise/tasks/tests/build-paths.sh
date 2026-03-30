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

run_tests() {
  local target_root
  target_root="$(mktemp -d)"
  trap "rm -rf '$target_root'" EXIT

  CARGO_TARGET_DIR="$target_root" \
    assert_eq \
      "$target_root/debug/codex" \
      "$(CARGO_TARGET_DIR="$target_root" resolve_codex_bin)" \
      "debug builds should install from active CARGO_TARGET_DIR"

  CARGO_TARGET_DIR="$target_root" \
    assert_eq \
      "$target_root/release/codex" \
      "$(CARGO_TARGET_DIR="$target_root" resolve_codex_bin --release)" \
      "release builds should install from active CARGO_TARGET_DIR"

  CARGO_TARGET_DIR="$target_root" \
    assert_eq \
      "$target_root/x86_64-unknown-linux-gnu/release/codex" \
      "$(CARGO_TARGET_DIR="$target_root" resolve_codex_bin --release --target x86_64-unknown-linux-gnu)" \
      "cross-target release builds should stay under active CARGO_TARGET_DIR"
}

run_tests

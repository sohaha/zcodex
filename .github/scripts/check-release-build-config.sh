#!/usr/bin/env bash
set -euo pipefail

required_vars=(
  CARGO_PROFILE_RELEASE_LTO
  CARGO_PROFILE_RELEASE_CODEGEN_UNITS
  CARGO_PROFILE_RELEASE_DEBUG
  CARGO_PROFILE_RELEASE_STRIP
)

check_make_target() {
  local target="$1"
  local expected="$2"
  local output
  output="$(make -n "$target")"

  for var in "${required_vars[@]}"; do
    if ! grep -q -- "-u ${var}" <<<"${output}"; then
      echo "make ${target} 缺少变量清理: ${var}" >&2
      exit 1
    fi
  done
  if ! grep -q -- "${expected}" <<<"${output}"; then
    echo "make ${target} 未调用预期 cargo release 构建命令" >&2
    exit 1
  fi
}

check_just_target() {
  local target="$1"
  local expected="$2"
  local target_body
  target_body="$(
    awk -v target="${target}" '
      $0 ~ "^" target " " { in_target=1; next }
      in_target && /^[^[:space:]]/ { exit }
      in_target { print }
    ' justfile
  )"

  if [[ -z "${target_body}" ]]; then
    echo "justfile 缺少 ${target} 目标" >&2
    exit 1
  fi

  for var in "${required_vars[@]}"; do
    if ! grep -q -- "-u ${var}" <<<"${target_body}"; then
      echo "just ${target} 缺少变量清理: ${var}" >&2
      exit 1
    fi
  done
  if ! grep -q -- "${expected}" <<<"${target_body}"; then
    echo "just ${target} 未调用预期 cargo release 构建命令" >&2
    exit 1
  fi
}

check_make_target "release-codex" "cargo build -p codex-cli --bin codex --release"
check_make_target "release-codex-serve" "cargo build -p codex-serve --bin codex-serve --release"

check_just_target "release-codex" "cargo build -p codex-cli --bin codex --release"
check_just_target "release-codex-serve" "cargo build -p codex-serve --bin codex-serve --release"

echo "release 构建入口校验通过"

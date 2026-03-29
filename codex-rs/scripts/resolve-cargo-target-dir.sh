#!/usr/bin/env bash
set -euo pipefail

slot="${1:-shared}"

if [ "${CODEX_CARGO_TARGET_EXACT:-0}" = "1" ] && [ -n "${CARGO_TARGET_DIR:-}" ]; then
  printf '%s\n' "$CARGO_TARGET_DIR"
  exit 0
fi

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
base_dir="${CARGO_TARGET_DIR_BASE:-${CARGO_TARGET_DIR:-$repo_root/.cargo-target}}"
lane="${CODEX_CARGO_LANE:-shared}"

sanitize() {
  printf '%s' "$1" | tr -c '[:alnum:]._-' '-'
}

slot="$(sanitize "$slot")"
lane="$(sanitize "$lane")"

if [ "$lane" = "shared" ]; then
  printf '%s/%s\n' "$base_dir" "$slot"
else
  printf '%s/%s/%s\n' "$base_dir" "$lane" "$slot"
fi

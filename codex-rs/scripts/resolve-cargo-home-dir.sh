#!/usr/bin/env bash
set -euo pipefail

slot="${1:-shared}"

if [ "${CODEX_CARGO_HOME_EXACT:-0}" = "1" ] && [ -n "${CARGO_HOME:-}" ]; then
  printf '%s\n' "$CARGO_HOME"
  exit 0
fi

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
base_dir="${CARGO_HOME_BASE:-${CARGO_HOME:-$repo_root/.cargo-home}}"
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

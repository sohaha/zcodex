#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/.." && pwd)"

if [ "${CODEX_DISABLE_SCCACHE:-0}" != "1" ] && command -v sccache >/dev/null 2>&1; then
  export SCCACHE_DIR="${SCCACHE_DIR:-$repo_root/.cache/sccache}"
  export SCCACHE_CACHE_SIZE="${SCCACHE_CACHE_SIZE:-50G}"
  mkdir -p "$SCCACHE_DIR"
  exec sccache "$@"
fi

exec "$@"

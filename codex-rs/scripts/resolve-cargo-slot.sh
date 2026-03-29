#!/usr/bin/env bash
set -euo pipefail

sanitize() {
  printf '%s' "$1" | tr -c '[:alnum:]._-' '-'
}

if [ -n "${CODEX_CARGO_SLOT:-}" ]; then
  printf '%s\n' "$(sanitize "$CODEX_CARGO_SLOT")"
  exit 0
fi

if [ "$#" -eq 0 ]; then
  printf '%s\n' "shared"
  exit 0
fi

slot=""
for arg in "$@"; do
  if [ -z "$slot" ]; then
    slot="$(sanitize "$arg")"
  else
    slot="${slot}_$(sanitize "$arg")"
  fi
done

if [ -z "$slot" ]; then
  slot="shared"
fi

printf '%s\n' "$slot"

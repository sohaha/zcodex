#!/usr/bin/env bash
set -euo pipefail

declare -a linker_cmd

if [ -n "${CC:-}" ]; then
  read -r -a linker_cmd <<<"$CC"
elif command -v clang >/dev/null 2>&1; then
  linker_cmd=("clang")
else
  linker_cmd=("cc")
fi

if command -v mold >/dev/null 2>&1; then
  exec "${linker_cmd[@]}" -fuse-ld=mold "$@"
fi

if command -v ld.lld >/dev/null 2>&1; then
  exec "${linker_cmd[@]}" -fuse-ld=lld "$@"
fi

exec "${linker_cmd[@]}" "$@"

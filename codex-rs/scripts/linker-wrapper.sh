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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
target_dir="${script_dir}/../../.cargo-target"
compat_obj="${target_dir}/isoc23-compat.o"

ensure_isoc23_compat() {
  if nm -D /usr/lib/x86_64-linux-gnu/libc.so.6 2>/dev/null | grep -q "__isoc23_strtol"; then
    return
  fi

  if [ ! -f "${compat_obj}" ]; then
    mkdir -p "${target_dir}"
    cat >"${target_dir}/isoc23-compat.c" <<'EOF'
#include <locale.h>
#include <stdlib.h>

long __isoc23_strtol(const char *nptr, char **endptr, int base) {
  return strtol(nptr, endptr, base);
}

unsigned long __isoc23_strtoul(const char *nptr, char **endptr, int base) {
  return strtoul(nptr, endptr, base);
}

long long __isoc23_strtoll(const char *nptr, char **endptr, int base) {
  return strtoll(nptr, endptr, base);
}

unsigned long long __isoc23_strtoull(const char *nptr, char **endptr, int base) {
  return strtoull(nptr, endptr, base);
}

long long __isoc23_strtoll_l(const char *nptr, char **endptr, int base, locale_t locale) {
  return strtoll_l(nptr, endptr, base, locale);
}

unsigned long long __isoc23_strtoull_l(const char *nptr, char **endptr, int base, locale_t locale) {
  return strtoull_l(nptr, endptr, base, locale);
}
EOF
    "${linker_cmd[0]}" -c "${target_dir}/isoc23-compat.c" -o "${compat_obj}"
  fi

  set -- "$@" "${compat_obj}"
  echo "$*"
}

extra_obj=""
if [ -f /usr/lib/x86_64-linux-gnu/libc.so.6 ] && ! nm -D /usr/lib/x86_64-linux-gnu/libc.so.6 2>/dev/null | grep -q "__isoc23_strtol"; then
  ensure_isoc23_compat
  extra_obj="${compat_obj}"
fi

if command -v mold >/dev/null 2>&1; then
  exec "${linker_cmd[@]}" -fuse-ld=mold "$@" ${extra_obj:+"$extra_obj"}
fi

if command -v ld.lld >/dev/null 2>&1; then
  exec "${linker_cmd[@]}" -fuse-ld=lld "$@" ${extra_obj:+"$extra_obj"}
fi

exec "${linker_cmd[@]}" "$@" ${extra_obj:+"$extra_obj"}

#!/usr/bin/env bash
set -euo pipefail

declare -a linker_cmd=()

if [ -n "${CC:-}" ]; then
  read -r -a linker_cmd <<<"$CC"
  # cargo-zigbuild exports `CC="zig cc ..."` for musl targets. Host-side GNU
  # build scripts still use this wrapper, and honoring that CC would link those
  # host binaries against musl instead of glibc.
  if [[ "${linker_cmd[*]}" == *"zig"* ]]; then
    linker_cmd=()
  fi
fi

if [ "${#linker_cmd[@]}" -eq 0 ] && command -v clang >/dev/null 2>&1; then
  linker_cmd=("clang")
else
  if [ "${#linker_cmd[@]}" -eq 0 ]; then
    linker_cmd=("cc")
  fi
fi

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
target_dir="${script_dir}/../../.cargo-target"
compat_obj="${target_dir}/isoc23-compat.o"
compat_lib_dir="${target_dir}/linker-lib-compat"

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

ensure_stdcpp_compat() {
  local system_lib="/usr/lib/x86_64-linux-gnu/libstdc++.so.6"
  local compat_lib="${compat_lib_dir}/libstdc++.so"

  if [ -f /usr/lib/x86_64-linux-gnu/libstdc++.so ] || [ ! -f "${system_lib}" ]; then
    return
  fi

  mkdir -p "${compat_lib_dir}"
  ln -sf "${system_lib}" "${compat_lib}"
}

extra_link_args=()
ensure_stdcpp_compat
if [ -f "${compat_lib_dir}/libstdc++.so" ]; then
  extra_link_args+=("-L${compat_lib_dir}")
fi

if command -v mold >/dev/null 2>&1; then
  exec "${linker_cmd[@]}" -fuse-ld=mold "$@" ${extra_obj:+"$extra_obj"} "${extra_link_args[@]}"
fi

if command -v ld.lld >/dev/null 2>&1; then
  exec "${linker_cmd[@]}" -fuse-ld=lld "$@" ${extra_obj:+"$extra_obj"} "${extra_link_args[@]}"
fi

exec "${linker_cmd[@]}" "$@" ${extra_obj:+"$extra_obj"} "${extra_link_args[@]}"

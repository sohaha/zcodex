#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
output_dir="${1:-$repo_root/codex-rs/dist/local-windows-cross}"
cross_dist_dir="${CODEX_WINDOWS_CROSS_DIST_DIR:-$repo_root/dist}"

if command -v zip >/dev/null 2>&1; then
  archive_cmd=zip
elif command -v 7z >/dev/null 2>&1; then
  archive_cmd=7z
else
  echo "[package_windows_cross_builds] 缺少 zip 或 7z，无法打包" >&2
  exit 1
fi

mkdir -p "$output_dir"
output_dir="$(cd "$output_dir" && pwd)"

package_one() {
  local source_path="$1"
  local packaged_name="$2"
  local archive_path="$output_dir/${packaged_name}.zip"
  local temp_dir

  if [ ! -f "$source_path" ]; then
    echo "[package_windows_cross_builds] 缺少构建产物: $source_path" >&2
    exit 1
  fi

  temp_dir="$(mktemp -d "${TMPDIR:-/tmp}/windows-cross-package.XXXXXX")"
  trap 'rm -rf "$temp_dir"' RETURN

  cp "$source_path" "$temp_dir/$packaged_name"
  rm -f "$archive_path"

  if [ "$archive_cmd" = "zip" ]; then
    (cd "$temp_dir" && zip -q "$archive_path" "$packaged_name")
  else
    (cd "$temp_dir" && 7z a -bd -tzip "$archive_path" "$packaged_name" >/dev/null)
  fi

  cp "$temp_dir/$packaged_name" "$output_dir/$packaged_name"
  trap - RETURN
  rm -rf "$temp_dir"
}

package_one \
  "$cross_dist_dir/codex-x86_64-pc-windows-msvc.exe" \
  "codex-x86_64-pc-windows-msvc.exe"

package_one \
  "$cross_dist_dir/codex-aarch64-pc-windows-gnullvm.exe" \
  "codex-aarch64-pc-windows-gnullvm.exe"

(
  cd "$output_dir"
  sha256sum \
    codex-x86_64-pc-windows-msvc.exe \
    codex-x86_64-pc-windows-msvc.exe.zip \
    codex-aarch64-pc-windows-gnullvm.exe \
    codex-aarch64-pc-windows-gnullvm.exe.zip \
    > SHA256SUMS.txt
)

echo "[package_windows_cross_builds] 打包完成: $output_dir" >&2

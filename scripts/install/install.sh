#!/bin/sh

# Usage:
#   sh install.sh [version]
#   CODEX_INSTALL_DIR=/path/to/bin sh install.sh [version]
# Script source:
#   https://github.com/sohaha/zcodex/scripts/install

set -eu

VERSION="${1:-latest}"
INSTALL_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"
BASE_URL="${CODEX_BASE_URL:-}"
path_action="already"
path_profile=""

step() {
  printf '==> %s\n' "$1"
}

normalize_version() {
  case "$1" in
    "" | latest)
      printf 'latest\n'
      ;;
    v*)
      printf '%s\n' "${1#v}"
      ;;
    *)
      printf '%s\n' "$1"
      ;;
  esac
}

download_file() {
  url="$1"
  output="$2"

  if command -v curl >/dev/null 2>&1; then
    curl -fL --progress-bar "$url" -o "$output"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q --show-progress -O "$output" "$url"
    return
  fi

  echo "curl or wget is required to install Codex." >&2
  exit 1
}

probe_remote_file() {
  url="$1"

  if command -v curl >/dev/null 2>&1; then
    curl -fsIL "$url" >/dev/null 2>&1
    return $?
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q --spider "$url" >/dev/null 2>&1
    return $?
  fi

  echo "curl or wget is required to install Codex." >&2
  exit 1
}

download_text() {
  url="$1"

  if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$url"
    return
  fi

  if command -v wget >/dev/null 2>&1; then
    wget -q -O - "$url"
    return
  fi

  echo "curl or wget is required to install Codex." >&2
  exit 1
}

add_to_path() {
  path_action="already"
  path_profile=""

  case ":$PATH:" in
    *":$INSTALL_DIR:"*)
      return
      ;;
  esac

  profile="$HOME/.profile"
  case "${SHELL:-}" in
    */zsh)
      profile="$HOME/.zshrc"
      ;;
    */bash)
      profile="$HOME/.bashrc"
      ;;
  esac

  path_profile="$profile"
  path_line="export PATH=\"$INSTALL_DIR:\$PATH\""
  if [ -f "$profile" ] && grep -F "$path_line" "$profile" >/dev/null 2>&1; then
    path_action="configured"
    return
  fi

  {
    printf '\n# Added by Codex installer\n'
    printf '%s\n' "$path_line"
  } >>"$profile"
  path_action="added"
}

release_url_for_asset() {
  asset="$1"
  resolved_version="$2"

  if [ -n "$BASE_URL" ]; then
    printf '%s/%s\n' "${BASE_URL%/}" "$asset"
    return
  fi

  printf 'https://github.com/sohaha/zcodex/releases/download/v%s/%s\n' "$resolved_version" "$asset"
}

copy_if_exists() {
  source_path="$1"
  dest_path="$2"

  if [ ! -f "$source_path" ]; then
    return 1
  fi

  cp "$source_path" "$dest_path"
  return 0
}

install_rg_if_available() {
  for rg_path in \
    "$tmp_dir/package/vendor/$primary_vendor_target/path/rg" \
    "$tmp_dir/package/vendor/$secondary_vendor_target/path/rg"
  do
    if [ -f "$rg_path" ]; then
      cp "$rg_path" "$INSTALL_DIR/rg"
      chmod 0755 "$INSTALL_DIR/rg"
      return
    fi
  done

  step "rg is not bundled in this release; keeping existing system rg if available"
}

install_from_npm_package() {
  tar -xzf "$archive_path" -C "$tmp_dir"

  step "Installing to $INSTALL_DIR"
  mkdir -p "$INSTALL_DIR"

  for codex_path in \
    "$tmp_dir/package/vendor/$primary_vendor_target/codex/codex" \
    "$tmp_dir/package/vendor/$secondary_vendor_target/codex/codex"
  do
    if copy_if_exists "$codex_path" "$INSTALL_DIR/codex"; then
      chmod 0755 "$INSTALL_DIR/codex"
      install_rg_if_available
      return
    fi
  done

  echo "Downloaded npm package does not contain a supported Codex binary layout." >&2
  exit 1
}

install_from_tarball_release() {
  archive_target="$1"

  tar -xzf "$archive_path" -C "$tmp_dir"

  step "Installing to $INSTALL_DIR"
  mkdir -p "$INSTALL_DIR"

  for codex_path in \
    "$tmp_dir/codex-$archive_target" \
    "$tmp_dir/codex"
  do
    if copy_if_exists "$codex_path" "$INSTALL_DIR/codex"; then
      chmod 0755 "$INSTALL_DIR/codex"
      step "Installed fallback archive format for target: $archive_target"
      step "rg is not bundled in fallback archives; install ripgrep separately if needed"
      return
    fi
  done

  echo "Downloaded archive does not contain an installable Codex binary." >&2
  exit 1
}

download_first_available_asset() {
  resolved_version="$1"
  shift

  for asset_name in "$@"; do
    asset_url="$(release_url_for_asset "$asset_name" "$resolved_version")"
    if probe_remote_file "$asset_url"; then
      downloaded_asset="$asset_name"
      download_file "$asset_url" "$archive_path"
      return 0
    fi
  done

  return 1
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required to install Codex." >&2
    exit 1
  fi
}

require_command mktemp
require_command tar

resolve_version() {
  normalized_version="$(normalize_version "$VERSION")"

  if [ "$normalized_version" != "latest" ]; then
    printf '%s\n' "$normalized_version"
    return
  fi

  release_json="$(download_text "https://api.github.com/repos/sohaha/zcodex/releases/latest")"
  resolved="$(printf '%s\n' "$release_json" | sed -n 's/.*"tag_name":[[:space:]]*"\(rust-\)\{0,1\}v\([^"]*\)".*/\2/p' | head -n 1)"

  if [ -z "$resolved" ]; then
    echo "Failed to resolve the latest Codex release version." >&2
    exit 1
  fi

  printf '%s\n' "$resolved"
}

case "$(uname -s)" in
  Darwin)
    os="darwin"
    ;;
  Linux)
    os="linux"
    ;;
  *)
    echo "install.sh supports macOS and Linux. Use install.ps1 on Windows." >&2
    exit 1
    ;;
esac

case "$(uname -m)" in
  x86_64 | amd64)
    arch="x86_64"
    ;;
  arm64 | aarch64)
    arch="aarch64"
    ;;
  *)
    echo "Unsupported architecture: $(uname -m)" >&2
    exit 1
    ;;
esac

if [ "$os" = "darwin" ] && [ "$arch" = "x86_64" ]; then
  if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || true)" = "1" ]; then
    arch="aarch64"
  fi
fi

if [ "$os" = "darwin" ]; then
  if [ "$arch" = "aarch64" ]; then
    npm_tag="darwin-arm64"
    primary_vendor_target="aarch64-apple-darwin"
    secondary_vendor_target="aarch64-apple-darwin"
    fallback_archive_target="aarch64-apple-darwin"
    platform_label="macOS (Apple Silicon)"
  else
    npm_tag="darwin-x64"
    primary_vendor_target="x86_64-apple-darwin"
    secondary_vendor_target="x86_64-apple-darwin"
    fallback_archive_target="x86_64-apple-darwin"
    platform_label="macOS (Intel)"
  fi
else
  if [ "$arch" = "aarch64" ]; then
    npm_tag="linux-arm64"
    primary_vendor_target="aarch64-unknown-linux-musl"
    secondary_vendor_target="aarch64-unknown-linux-gnu"
    fallback_archive_target="aarch64-unknown-linux-gnu"
    platform_label="Linux (ARM64)"
  else
    npm_tag="linux-x64"
    primary_vendor_target="x86_64-unknown-linux-musl"
    secondary_vendor_target="x86_64-unknown-linux-gnu"
    fallback_archive_target="x86_64-unknown-linux-gnu"
    platform_label="Linux (x64)"
  fi
fi

if [ -x "$INSTALL_DIR/codex" ]; then
  install_mode="Updating"
else
  install_mode="Installing"
fi

step "$install_mode Codex CLI"
step "Detected platform: $platform_label"

resolved_version="$(resolve_version)"
npm_asset="codex-npm-$npm_tag-$resolved_version.tgz"
fallback_archive_asset="codex-$fallback_archive_target.tar.gz"
downloaded_asset=""

step "Resolved version: $resolved_version"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT INT TERM

archive_path="$tmp_dir/codex-download"

step "Downloading Codex CLI"
if download_first_available_asset "$resolved_version" "$npm_asset"; then
  step "Using release asset: $downloaded_asset"
  install_from_npm_package
elif download_first_available_asset "$resolved_version" "$fallback_archive_asset"; then
  step "Using fallback release asset: $downloaded_asset"
  install_from_tarball_release "$fallback_archive_target"
else
  echo "Failed to find a supported Codex release asset for $platform_label." >&2
  echo "Tried assets: $npm_asset, $fallback_archive_asset" >&2
  exit 1
fi

add_to_path

case "$path_action" in
  added)
    step "PATH updated for future shells in $path_profile"
    step "Run now: export PATH=\"$INSTALL_DIR:\$PATH\" && codex"
    step "Or open a new terminal and run: codex"
    ;;
  configured)
    step "PATH is already configured for future shells in $path_profile"
    step "Run now: export PATH=\"$INSTALL_DIR:\$PATH\" && codex"
    step "Or open a new terminal and run: codex"
    ;;
  *)
    step "$INSTALL_DIR is already on PATH"
    step "Run: codex"
    ;;
esac

printf 'Codex CLI %s installed successfully.\n' "$resolved_version"

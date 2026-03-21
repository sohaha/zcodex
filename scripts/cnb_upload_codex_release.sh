#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
target_dir="${CODEX_TARGET_DIR:-$repo_root/codex-rs/target}"
dist_dir="${CODEX_RELEASE_DIST_DIR:-$repo_root/codex-rs/dist/cnb-release}"
release_tag="${RELEASE_TAG:-${CNB_BRANCH:-}}"
github_repo="${GITHUB_REPOSITORY:-${RELEASE_REPOSITORY:-sohaha/zcodex}}"
dry_run="${DRY_RUN:-0}"
current_commit="$(git -C "$repo_root" rev-parse HEAD)"

if [ -z "$release_tag" ]; then
  release_tag="$(git -C "$repo_root" describe --tags --exact-match 2>/dev/null || true)"
fi

if [ -z "$release_tag" ]; then
  echo "[cnb-release] missing release tag; set RELEASE_TAG or CNB_BRANCH" >&2
  exit 1
fi

mkdir -p "$dist_dir"
rm -f "$dist_dir"/*

stage_asset() {
  local source_path="$1"
  local asset_name="$2"

  if [ ! -f "$source_path" ]; then
    echo "[cnb-release] missing asset source: $source_path" >&2
    exit 1
  fi

  cp "$source_path" "$dist_dir/$asset_name"
  echo "[cnb-release] staged $asset_name" >&2
}

stage_asset \
  "$target_dir/x86_64-unknown-linux-gnu/release/codex" \
  "codex-x86_64-unknown-linux-gnu"
stage_asset \
  "$target_dir/aarch64-apple-darwin/release/codex" \
  "codex-aarch64-apple-darwin"
stage_asset \
  "$target_dir/x86_64-pc-windows-gnu/release/codex.exe" \
  "codex-x86_64-pc-windows-gnu.exe"
stage_asset \
  "$target_dir/aarch64-pc-windows-gnullvm/release/codex.exe" \
  "codex-aarch64-pc-windows-gnullvm.exe"

echo "[cnb-release] release tag: $release_tag" >&2
echo "[cnb-release] github repo: $github_repo" >&2
echo "[cnb-release] dist dir: $dist_dir" >&2
find "$dist_dir" -maxdepth 1 -type f -printf '[cnb-release] asset %f\n' >&2

if [ "$dry_run" = "1" ]; then
  echo "[cnb-release] dry-run enabled; skip GitHub Release upload" >&2
  exit 0
fi

if [ -z "${GITHUB_TOKEN:-}" ]; then
  echo "[cnb-release] missing GITHUB_TOKEN" >&2
  exit 1
fi

if ! command -v gh >/dev/null 2>&1; then
  echo "[cnb-release] missing gh cli" >&2
  exit 1
fi

export GH_TOKEN="$GITHUB_TOKEN"

resolve_remote_tag_commit() {
  local remote_url="$1"
  local tag_name="$2"

  git ls-remote "$remote_url" "refs/tags/$tag_name" "refs/tags/$tag_name^{}" \
    | awk -v tag_name="$tag_name" '
        $2 == "refs/tags/" tag_name "^{}" {
          print $1
          found = 1
          exit
        }
        $2 == "refs/tags/" tag_name && fallback == "" {
          fallback = $1
        }
        END {
          if (!found && fallback != "") {
            print fallback
          }
        }
      '
}

github_push_url="https://x-access-token:${GITHUB_TOKEN}@github.com/${github_repo}.git"
github_read_url="https://github.com/${github_repo}.git"
remote_tag_commit="$(resolve_remote_tag_commit "$github_read_url" "$release_tag")"

if [ -z "$remote_tag_commit" ]; then
  git -C "$repo_root" push "$github_push_url" "HEAD:refs/tags/$release_tag"
elif [ "$remote_tag_commit" != "$current_commit" ]; then
  cat >&2 <<EOF
[cnb-release] remote tag mismatch for $release_tag
[cnb-release] current commit: $current_commit
[cnb-release] remote commit:  $remote_tag_commit
EOF
  exit 1
fi

if ! gh release view "$release_tag" --repo "$github_repo" >/dev/null 2>&1; then
  create_args=(
    "$release_tag"
    --repo "$github_repo"
    --title "$release_tag"
    --notes ""
  )
  case "$release_tag" in
    *-alpha*|*-beta*)
      create_args+=(--prerelease)
      ;;
  esac
  gh release create "${create_args[@]}"
fi

gh release upload "$release_tag" "$dist_dir"/* --repo "$github_repo" --clobber

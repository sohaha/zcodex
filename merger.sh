#!/usr/bin/env bash
set -euo pipefail

upstream_remote="${UPSTREAM_REMOTE:-nullclaw_upstream_temp}"
upstream_url="${UPSTREAM_URL:-https://github.com/openai/codex}"
opencode_bin="${OPENCODE_BIN:-opencode}"
base_commit="$(git rev-parse HEAD)"
remote_added=0

cleanup() {
  if [[ "${remote_added}" -eq 1 ]]; then
    git remote remove "${upstream_remote}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if existing_url="$(git remote get-url "${upstream_remote}" 2>/dev/null)"; then
  if [[ "${existing_url}" != "${upstream_url}" ]]; then
    echo "远程 ${upstream_remote} 已存在且 URL 不匹配: ${existing_url}"
    echo "请改用 UPSTREAM_REMOTE 或 UPSTREAM_URL"
    exit 1
  fi
else
  git remote add "${upstream_remote}" "${upstream_url}"
  remote_added=1
fi

git fetch "${upstream_remote}"
git merge "${upstream_remote}/main"

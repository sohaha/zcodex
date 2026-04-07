#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(git -C "$script_dir/../../.." rev-parse --show-toplevel)"

assert_contains() {
  local haystack="$1"
  local needle="$2"
  local message="$3"

  if [[ "$haystack" != *"$needle"* ]]; then
    echo "assertion failed: $message" >&2
    echo "  expected to find: $needle" >&2
    echo "  actual output: $haystack" >&2
    exit 1
  fi
}

run_tests() {
  local temp_root
  local task_dir
  local dist_bin
  local args_file
  local output

  temp_root="$(mktemp -d)"
  trap "rm -rf '$temp_root'" EXIT
  task_dir="$temp_root/tasks"
  dist_bin="$temp_root/target/release/codex"
  args_file="$temp_root/build-args"

  mkdir -p "$task_dir"
  cp "$repo_root/.mise/tasks/build-release" "$task_dir/build-release"
  cp "$repo_root/.mise/tasks/lib-artifact-size" "$task_dir/lib-artifact-size"

  cat >"$task_dir/build" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$script_dir/lib-artifact-size"

resolve_codex_bin() {
  printf '%s\n' "$TEST_DIST_BIN"
}

main() {
  printf '%s\n' "$*" >"$TEST_BUILD_ARGS_FILE"
  mkdir -p "$(dirname "$TEST_DIST_BIN")"
  printf '#!/usr/bin/env bash\nexit 0\n' >"$TEST_DIST_BIN"
  chmod +x "$TEST_DIST_BIN"
  echo "[build] stub build ran" >&2
}

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
  main "$@"
fi
EOF

  chmod +x "$task_dir/build-release" "$task_dir/build"

  output="$(
    TEST_DIST_BIN="$dist_bin" \
      TEST_BUILD_ARGS_FILE="$args_file" \
      CNB_VSCODE_REMOTE_SSH_SCHEMA="vscode://vscode-remote/ssh-remote+user@example.com/workspace" \
      bash "$task_dir/build-release" --target x86_64-unknown-linux-gnu 2>&1
  )"

  assert_contains \
    "$output" \
    "[build-release] 二进制路径: $dist_bin" \
    "build-release should resolve the built artifact path without parsing build stderr"

  assert_contains \
    "$output" \
    "scp user@example.com:$dist_bin ./codex" \
    "build-release should print the remote download hint when CNB remote SSH metadata exists"

  assert_contains \
    "$(cat "$args_file")" \
    "--target x86_64-unknown-linux-gnu --release" \
    "build-release should append --release when the caller omitted it"
}

run_tests

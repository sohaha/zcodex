#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/macos_sign_and_notarize_local.sh \
    --identity "Developer ID Application: Example, Inc. (TEAMID)" \
    --binary /path/to/codex \
    --p8 /path/to/AuthKey_ABC123XYZ.p8 \
    --key-id ABC123XYZ \
    --issuer-id 00000000-0000-0000-0000-000000000000

Options:
  --identity NAME      codesign identity name or SHA-1 hash from keychain
  --installer-identity NAME
                       Required when signing .pkg; use a Developer ID Installer identity
  --binary PATH        Sign a standalone Mach-O binary; repeatable
  --app PATH           Sign and notarize a .app bundle; repeatable
  --dmg PATH           Sign and notarize a .dmg; repeatable
  --pkg PATH           Sign and notarize a .pkg; repeatable
  --entitlements PATH  Optional entitlements plist for app/binary signing
  --p8 PATH            App Store Connect API key (.p8)
  --key-id ID          App Store Connect API key ID
  --issuer-id ID       App Store Connect issuer ID
  --team-id ID         Optional team ID for verification output only
  --no-notarize        Sign only; skip notarization and stapling
  --dry-run            Print planned actions without executing them
  -h, --help           Show this help

Examples:
  scripts/macos_sign_and_notarize_local.sh \
    --identity "Developer ID Application: Example, Inc. (TEAMID)" \
    --binary ./codex \
    --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
    --key-id ABC123XYZ \
    --issuer-id 00000000-0000-0000-0000-000000000000

  scripts/macos_sign_and_notarize_local.sh \
    --identity ABCDEF0123456789ABCDEF0123456789ABCDEF01 \
    --app ./MyApp.app \
    --dmg ./MyApp.dmg \
    --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
    --key-id ABC123XYZ \
    --issuer-id 00000000-0000-0000-0000-000000000000

  scripts/macos_sign_and_notarize_local.sh \
    --identity "Developer ID Application: Example, Inc. (TEAMID)" \
    --installer-identity "Developer ID Installer: Example, Inc. (TEAMID)" \
    --pkg ./MyApp.pkg \
    --p8 ~/keys/AuthKey_ABC123XYZ.p8 \
    --key-id ABC123XYZ \
    --issuer-id 00000000-0000-0000-0000-000000000000
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

run() {
  if [ "$dry_run" -eq 1 ]; then
    printf 'dry-run:'
    printf ' %q' "$@"
    printf '\n'
    return
  fi

  "$@"
}

append_target() {
  local kind="$1"
  local path="$2"

  [ -e "$path" ] || die "$kind target not found: $path"
  targets_kind+=("$kind")
  targets_path+=("$path")
}

validate_id_like() {
  local label="$1"
  local value="$2"
  local pattern="$3"
  [[ "$value" =~ $pattern ]] || die "$label format looks wrong: $value"
}

sign_target() {
  local kind="$1"
  local path="$2"
  local -a args
  local signed_pkg=""

  if [ "$kind" = "pkg" ]; then
    if [ "$dry_run" -eq 1 ]; then
      printf 'dry-run:'
      printf ' %q' productsign --sign "$installer_identity" --timestamp "$path" "${path}.signed"
      printf '\n'
      return
    fi

    signed_pkg="$(mktemp "${TMPDIR:-/tmp}/signed-installer.XXXXXX.pkg")"
    productsign --sign "$installer_identity" --timestamp "$path" "$signed_pkg"
    mv "$signed_pkg" "$path"
    return
  fi

  args=(
    codesign
    --force
    --timestamp
    --sign "$identity"
  )

  if [ "$kind" != "pkg" ]; then
    args+=(--options runtime)
  fi

  if [ -n "$entitlements" ] && [ "$kind" != "dmg" ] && [ "$kind" != "pkg" ]; then
    args+=(--entitlements "$entitlements")
  fi

  args+=("$path")
  run "${args[@]}"
}

verify_signature() {
  local kind="$1"
  local path="$2"

  if [ "$kind" = "pkg" ]; then
    run pkgutil --check-signature "$path"
    run spctl -a -vv -t install "$path"
    return
  fi

  run codesign --verify --verbose=4 "$path"
  run spctl -a -vv "$path"
}

create_notary_payload() {
  local kind="$1"
  local path="$2"
  local out_var="$3"
  local archive

  case "$kind" in
    binary)
      archive="$(mktemp "${TMPDIR:-/tmp}/notary-binary.XXXXXX.zip")"
      run ditto -c -k --keepParent "$path" "$archive"
      printf -v "$out_var" '%s' "$archive"
      ;;
    app|dmg|pkg)
      printf -v "$out_var" '%s' "$path"
      ;;
    *)
      die "unsupported target kind for notarization: $kind"
      ;;
  esac
}

notarize_target() {
  local kind="$1"
  local path="$2"
  local payload=""
  local submission_json=""
  local submission_id=""
  local status=""
  local archive_created=0

  create_notary_payload "$kind" "$path" payload
  if [ "$payload" != "$path" ]; then
    archive_created=1
  fi

  if [ "$dry_run" -eq 1 ]; then
    printf 'dry-run:'
    printf ' %q' xcrun notarytool submit "$payload" --key "$p8_path" --key-id "$key_id" --issuer "$issuer_id" --wait --output-format json
    printf '\n'
    return
  fi

  submission_json="$(
    xcrun notarytool submit "$payload" \
      --key "$p8_path" \
      --key-id "$key_id" \
      --issuer "$issuer_id" \
      --wait \
      --output-format json
  )"

  status="$(printf '%s\n' "$submission_json" | jq -r '.status // "Unknown"')"
  submission_id="$(printf '%s\n' "$submission_json" | jq -r '.id // ""')"
  [ -n "$submission_id" ] || die "failed to retrieve notarization submission id for $path"

  echo "notarization completed: $path status=$status id=$submission_id" >&2

  if [ "$status" != "Accepted" ]; then
    xcrun notarytool log "$submission_id" \
      --key "$p8_path" \
      --key-id "$key_id" \
      --issuer "$issuer_id" >&2 || true
    die "notarization failed for $path"
  fi

  if [ "$archive_created" -eq 1 ]; then
    rm -f "$payload"
  fi
}

staple_target() {
  local kind="$1"
  local path="$2"

  case "$kind" in
    app|dmg|pkg)
      run xcrun stapler staple "$path"
      ;;
    binary)
      echo "skip staple for standalone binary: $path" >&2
      ;;
    *)
      die "unsupported target kind for stapling: $kind"
      ;;
  esac
}

main() {
  identity=""
  installer_identity=""
  entitlements=""
  p8_path=""
  key_id=""
  issuer_id=""
  team_id=""
  notarize=1
  dry_run=0
  targets_kind=()
  targets_path=()

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --identity)
        shift
        [ "$#" -gt 0 ] || die "missing value for --identity"
        identity="$1"
        ;;
      --installer-identity)
        shift
        [ "$#" -gt 0 ] || die "missing value for --installer-identity"
        installer_identity="$1"
        ;;
      --binary)
        shift
        [ "$#" -gt 0 ] || die "missing value for --binary"
        append_target "binary" "$1"
        ;;
      --app)
        shift
        [ "$#" -gt 0 ] || die "missing value for --app"
        append_target "app" "$1"
        ;;
      --dmg)
        shift
        [ "$#" -gt 0 ] || die "missing value for --dmg"
        append_target "dmg" "$1"
        ;;
      --pkg)
        shift
        [ "$#" -gt 0 ] || die "missing value for --pkg"
        append_target "pkg" "$1"
        ;;
      --entitlements)
        shift
        [ "$#" -gt 0 ] || die "missing value for --entitlements"
        entitlements="$1"
        ;;
      --p8)
        shift
        [ "$#" -gt 0 ] || die "missing value for --p8"
        p8_path="$1"
        ;;
      --key-id)
        shift
        [ "$#" -gt 0 ] || die "missing value for --key-id"
        key_id="$1"
        ;;
      --issuer-id)
        shift
        [ "$#" -gt 0 ] || die "missing value for --issuer-id"
        issuer_id="$1"
        ;;
      --team-id)
        shift
        [ "$#" -gt 0 ] || die "missing value for --team-id"
        team_id="$1"
        ;;
      --no-notarize)
        notarize=0
        ;;
      --dry-run)
        dry_run=1
        ;;
      -h|--help)
        usage
        exit 0
        ;;
      *)
        die "unknown argument: $1"
        ;;
    esac
    shift
  done

  [ -n "$identity" ] || die "--identity is required"
  [ "${#targets_path[@]}" -gt 0 ] || die "at least one target is required"

  if [ -n "$entitlements" ]; then
    [ -f "$entitlements" ] || die "entitlements file not found: $entitlements"
  fi

  require_cmd codesign
  require_cmd xcrun
  require_cmd spctl
  require_cmd ditto
  require_cmd jq
  require_cmd security

  local has_pkg_target=0
  local kind
  for kind in "${targets_kind[@]}"; do
    if [ "$kind" = "pkg" ]; then
      has_pkg_target=1
      break
    fi
  done

  if [ "$has_pkg_target" -eq 1 ]; then
    [ -n "$installer_identity" ] || die "--installer-identity is required when signing .pkg targets"
    require_cmd productsign
    require_cmd pkgutil
  fi

  if [ "$notarize" -eq 1 ]; then
    [ -f "$p8_path" ] || die "--p8 file not found: $p8_path"
    [ -n "$key_id" ] || die "--key-id is required unless --no-notarize is used"
    [ -n "$issuer_id" ] || die "--issuer-id is required unless --no-notarize is used"
    validate_id_like "key id" "$key_id" '^[A-Z0-9]{10}$'
    validate_id_like "issuer id" "$issuer_id" '^[0-9A-Fa-f-]{36}$'
  fi

  echo "signing identity: $identity" >&2
  if [ -n "$team_id" ]; then
    echo "team id: $team_id" >&2
  fi
  security find-identity -v -p codesigning >&2 || true

  local i
  for i in "${!targets_path[@]}"; do
    echo "signing ${targets_kind[$i]}: ${targets_path[$i]}" >&2
    sign_target "${targets_kind[$i]}" "${targets_path[$i]}"
    verify_signature "${targets_kind[$i]}" "${targets_path[$i]}"
  done

  if [ "$notarize" -eq 0 ]; then
    echo "sign-only mode completed" >&2
    exit 0
  fi

  for i in "${!targets_path[@]}"; do
    echo "notarizing ${targets_kind[$i]}: ${targets_path[$i]}" >&2
    notarize_target "${targets_kind[$i]}" "${targets_path[$i]}"
    staple_target "${targets_kind[$i]}" "${targets_path[$i]}"
  done

  echo "all targets signed and notarized successfully" >&2
}

main "$@"

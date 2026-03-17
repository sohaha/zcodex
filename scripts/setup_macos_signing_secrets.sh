#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  scripts/setup_macos_signing_secrets.sh \
    --p12 /path/to/developer-id.p12 \
    --p8 /path/to/AuthKey_ABC123XYZ.p8 \
    --key-id ABC123XYZ \
    --issuer-id 00000000-0000-0000-0000-000000000000

Options:
  --p12 PATH          Developer ID Application certificate exported as .p12
  --p8 PATH           App Store Connect API key (.p8)
  --key-id ID         App Store Connect API key ID
  --issuer-id ID      App Store Connect issuer ID (UUID)
  --repo OWNER/NAME   Target GitHub repository; defaults to current gh repo
  --dry-run           Validate inputs and print actions without uploading secrets
  -h, --help          Show this help

Environment:
  APPLE_CERTIFICATE_PASSWORD   Password for the .p12 file. If unset, the script prompts.

Secrets written:
  APPLE_CERTIFICATE_P12
  APPLE_CERTIFICATE_PASSWORD
  APPLE_NOTARIZATION_KEY_P8
  APPLE_NOTARIZATION_KEY_ID
  APPLE_NOTARIZATION_ISSUER_ID
EOF
}

die() {
  echo "error: $*" >&2
  exit 1
}

require_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

base64_no_wrap() {
  base64 < "$1" | tr -d '\n'
}

set_secret() {
  local repo="$1"
  local name="$2"
  local value="$3"

  gh secret set "$name" --repo "$repo" --body "$value" >/dev/null
  echo "wrote secret: $name" >&2
}

prompt_password() {
  local password

  read -r -s -p "APPLE_CERTIFICATE_PASSWORD: " password
  echo >&2
  [ -n "$password" ] || die "empty APPLE_CERTIFICATE_PASSWORD"
  printf '%s' "$password"
}

validate_uuid_like() {
  local value="$1"
  [[ "$value" =~ ^[0-9A-Fa-f-]{36}$ ]] || die "issuer id must look like a UUID"
}

validate_key_id_like() {
  local value="$1"
  [[ "$value" =~ ^[A-Z0-9]{10}$ ]] || die "key id must look like Apple's 10-char ID"
}

main() {
  local p12_path=""
  local p8_path=""
  local key_id=""
  local issuer_id=""
  local repo=""
  local dry_run=0
  local cert_password="${APPLE_CERTIFICATE_PASSWORD:-}"
  local cert_b64=""
  local key_b64=""

  while [ "$#" -gt 0 ]; do
    case "$1" in
      --p12)
        shift
        [ "$#" -gt 0 ] || die "missing value for --p12"
        p12_path="$1"
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
      --repo)
        shift
        [ "$#" -gt 0 ] || die "missing value for --repo"
        repo="$1"
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

  [ -n "$p12_path" ] || die "--p12 is required"
  [ -n "$p8_path" ] || die "--p8 is required"
  [ -n "$key_id" ] || die "--key-id is required"
  [ -n "$issuer_id" ] || die "--issuer-id is required"
  [ -f "$p12_path" ] || die ".p12 file not found: $p12_path"
  [ -f "$p8_path" ] || die ".p8 file not found: $p8_path"
  validate_key_id_like "$key_id"
  validate_uuid_like "$issuer_id"

  require_cmd base64
  require_cmd tr

  if [ -z "$repo" ]; then
    require_cmd gh
    repo="$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
  fi
  [ -n "$repo" ] || die "failed to resolve GitHub repo"

  if [ -z "$cert_password" ]; then
    cert_password="$(prompt_password)"
  fi

  cert_b64="$(base64_no_wrap "$p12_path")"
  key_b64="$(base64_no_wrap "$p8_path")"

  echo "target repo: $repo" >&2
  echo "p12 file: $p12_path" >&2
  echo "p8 file: $p8_path" >&2
  echo "key id: $key_id" >&2
  echo "issuer id: $issuer_id" >&2

  if [ "$dry_run" -eq 1 ]; then
    echo "dry-run: would write secrets:" >&2
    echo "  APPLE_CERTIFICATE_P12" >&2
    echo "  APPLE_CERTIFICATE_PASSWORD" >&2
    echo "  APPLE_NOTARIZATION_KEY_P8" >&2
    echo "  APPLE_NOTARIZATION_KEY_ID" >&2
    echo "  APPLE_NOTARIZATION_ISSUER_ID" >&2
    exit 0
  fi

  require_cmd gh
  gh auth status >/dev/null
  set_secret "$repo" "APPLE_CERTIFICATE_P12" "$cert_b64"
  set_secret "$repo" "APPLE_CERTIFICATE_PASSWORD" "$cert_password"
  set_secret "$repo" "APPLE_NOTARIZATION_KEY_P8" "$key_b64"
  set_secret "$repo" "APPLE_NOTARIZATION_KEY_ID" "$key_id"
  set_secret "$repo" "APPLE_NOTARIZATION_ISSUER_ID" "$issuer_id"

  echo "done: macOS signing secrets uploaded for $repo" >&2
}

main "$@"

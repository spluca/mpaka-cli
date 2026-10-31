#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bucket="${MPAKA_CLI_R2_BUCKET:-mpaka-cli-releases}"

die() {
  printf 'cli-bridge-publish: %s\n' "$*" >&2
  exit 1
}

fetch_status() {
  local url="$1" destination="$2" status rc
  set +e
  status="$(curl \
    --silent \
    --show-error \
    --proto '=https' \
    --proto-redir '=https' \
    --max-redirs 0 \
    --max-time 60 \
    --output "$destination" \
    --write-out '%{http_code}' \
    "$url")"
  rc=$?
  set -e
  [[ "$rc" -eq 0 ]] || die "public object probe failed for $url"
  [[ "$status" =~ ^[0-9]{3}$ ]] || die "public object probe returned an invalid status for $url"
  printf '%s' "$status"
}

put_object() {
  local file="$1" key="$2" content_type="$3"
  pnpm --dir "$root/cli-distribution" exec wrangler r2 object put "$bucket/$key" \
    --file "$file" \
    --remote \
    --content-type "$content_type"
}

reconcile_immutable_object() {
  local file="$1" key="$2" url="$3" content_type="$4" existing status
  [[ -f "$file" ]] || die "local release object is missing: $file"
  existing="$(mktemp)"
  trap 'rm -f "$existing"' RETURN
  status="$(fetch_status "$url" "$existing")"
  case "$status" in
    200)
      cmp --silent "$file" "$existing" || die "immutable object differs: $key"
      ;;
    404)
      put_object "$file" "$key" "$content_type"
      status="$(fetch_status "$url" "$existing")"
      [[ "$status" == 200 ]] || die "published object is not publicly readable: $key"
      cmp --silent "$file" "$existing" || die "published object differs: $key"
      ;;
    *) die "indeterminate public object status $status for $key" ;;
  esac
  rm -f "$existing"
  trap - RETURN
}

publish_release() {
  local bundle="$1" base_url="$2" manifest filename
  manifest="$bundle/manifest.json"
  [[ -f "$manifest" && -f "$bundle/manifest.json.sig" ]] || die 'bridge manifest or signature is missing'
  [[ "$(jq -r '.version' "$manifest")" == v0.2.0 ]] || die 'bridge manifest version must be v0.2.0'
  [[ "$(jq '.artifacts | length' "$manifest")" -eq 6 ]] || die 'bridge manifest must contain six artifacts'

  while IFS= read -r filename; do
    reconcile_immutable_object \
      "$bundle/$filename" \
      "releases/v0.2.0/$filename" \
      "$base_url/releases/v0.2.0/$filename" \
      application/octet-stream
  done < <(jq -r '.artifacts[].filename' "$manifest")
  reconcile_immutable_object \
    "$bundle/manifest.json.sig" \
    releases/v0.2.0/manifest.json.sig \
    "$base_url/releases/v0.2.0/manifest.json.sig" \
    application/octet-stream
  reconcile_immutable_object \
    "$manifest" \
    releases/v0.2.0/manifest.json \
    "$base_url/releases/v0.2.0/manifest.json" \
    'application/json; charset=utf-8'
}

publish_installers() {
  local bundle="$1" base_url="$2" installer public_copy status
  for installer in install install.ps1; do
    [[ -f "$bundle/$installer" ]] || die "prepared installer is missing: $installer"
    put_object "$bundle/$installer" "$installer" 'text/plain; charset=utf-8'
    public_copy="$(mktemp)"
    trap 'rm -f "$public_copy"' RETURN
    status="$(fetch_status "$base_url/$installer" "$public_copy")"
    [[ "$status" == 200 ]] || die "published installer is not publicly readable: $installer"
    cmp --silent "$bundle/$installer" "$public_copy" || die "published installer differs: $installer"
    rm -f "$public_copy"
    trap - RETURN
  done
}

command_name="${1:-}"
shift || true
bundle=''
base_url=''
while (($#)); do
  case "$1" in
    --bundle) bundle="${2:-}"; shift 2 ;;
    --base-url) base_url="${2:-}"; shift 2 ;;
    *) die "unexpected argument: $1" ;;
  esac
done
[[ -d "$bundle" ]] || die 'bundle directory does not exist'
[[ "$base_url" == https://* ]] || die 'base URL must use HTTPS'
base_url="${base_url%/}"

case "$command_name" in
  release) publish_release "$bundle" "$base_url" ;;
  installers) publish_installers "$bundle" "$base_url" ;;
  *) die 'usage: cli-bridge-publish.sh release|installers --bundle DIR --base-url HTTPS_URL' ;;
esac

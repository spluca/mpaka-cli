#!/usr/bin/env bash
set -Eeuo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source-path=SCRIPTDIR
source "$script_dir/cli-public-acceptance-lib.sh"

base_url="${MPAKA_CLI_PUBLIC_BASE_URL:-https://cli.mpaka.dev}"
legacy_public_key="${MPAKA_CLI_LEGACY_PUBLIC_KEY_FILE:-}"
active_public_key="${MPAKA_CLI_ACTIVE_PUBLIC_KEY_FILE:-${MPAKA_CLI_PUBLIC_KEY_FILE:-}}"
[[ "$base_url" == https://* ]] || { echo 'MPAKA_CLI_PUBLIC_BASE_URL must use HTTPS' >&2; exit 1; }
[[ -r "$legacy_public_key" ]] || { echo 'MPAKA_CLI_LEGACY_PUBLIC_KEY_FILE must point to the old release public key' >&2; exit 1; }
[[ -r "$active_public_key" ]] || { echo 'MPAKA_CLI_ACTIVE_PUBLIC_KEY_FILE must point to the active release public key' >&2; exit 1; }
command -v curl >/dev/null || { echo 'curl is required' >&2; exit 1; }
command -v jq >/dev/null || { echo 'jq is required' >&2; exit 1; }
command -v openssl >/dev/null || { echo 'openssl is required' >&2; exit 1; }
command -v sha256sum >/dev/null || { echo 'sha256sum is required' >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

get_public() {
  local url="$1" output="$2" headers="$3" status
  status="$(curl --silent --show-error --fail --proto '=https' --proto-redir '=https' --max-redirs 0 --dump-header "$headers" --output "$output" --write-out '%{http_code}' "$url")" || {
    echo "public request failed: $url" >&2
    exit 1
  }
  [[ "$status" =~ ^2[0-9][0-9]$ ]] || { echo "expected public 2xx for $url, got $status" >&2; exit 1; }
}

get_public "$base_url/install" "$work/install" "$work/install.headers"
get_public "$base_url/install.ps1" "$work/install.ps1" "$work/install-ps.headers"
get_public "$base_url/latest" "$work/latest" "$work/latest.headers"
get_public "$base_url/latest-v2" "$work/latest-v2" "$work/latest-v2.headers"
grep -q '^#!/usr/bin/env bash' "$work/install"
grep -q 'param(' "$work/install.ps1"

compatibility_version="$(tr -d '\r\n' < "$work/latest")"
active_version="$(tr -d '\r\n' < "$work/latest-v2")"
require_compatibility_pointer "$compatibility_version" "$active_version"

verify_release() {
  local version="$1" public_key="$2" label="$3" release_dir manifest signature
  release_dir="$work/release-$version"
  manifest="$release_dir/manifest.json"
  signature="$release_dir/manifest.json.sig"
  mkdir -p "$release_dir"
  if [[ ! -f "$manifest" ]]; then
    get_public "$base_url/releases/$version/manifest.json" "$manifest" "$release_dir/manifest.headers"
    get_public "$base_url/releases/$version/manifest.json.sig" "$signature" "$release_dir/manifest-sig.headers"
  fi
  openssl dgst -sha256 -verify "$public_key" -signature "$signature" "$manifest" >/dev/null \
    || { echo "$label manifest signature verification failed" >&2; exit 1; }
  [[ "$(jq -r '.version' "$manifest")" == "$version" ]] || { echo "$label manifest version mismatch" >&2; exit 1; }
  [[ "$(jq '.artifacts | length' "$manifest")" == 6 ]] || { echo "$label manifest does not contain six artifacts" >&2; exit 1; }
  if [[ ! -f "$release_dir/artifacts.verified" ]]; then
    while IFS=$'\t' read -r filename expected_size expected_hash; do
      get_public "$base_url/releases/$version/$filename" "$release_dir/$filename" "$release_dir/$filename.headers"
      [[ "$(wc -c < "$release_dir/$filename" | tr -d '[:space:]')" == "$expected_size" ]] || { echo "$label artifact size mismatch: $filename" >&2; exit 1; }
      [[ "$(sha256sum "$release_dir/$filename" | awk '{print $1}')" == "$expected_hash" ]] || { echo "$label artifact hash mismatch: $filename" >&2; exit 1; }
    done < <(jq -r '.artifacts[] | [.filename, (.size | tostring), .sha256] | @tsv' "$manifest")
    : > "$release_dir/artifacts.verified"
  fi
}

verify_release "$compatibility_version" "$legacy_public_key" compatibility
verify_release "$active_version" "$active_public_key" active

landing_base="${MPAKA_LANDING_BASE_URL:-https://mpaka.dev}"
landing_status="$(curl --silent --show-error --proto '=https' --proto-redir '=https' --max-redirs 0 --dump-header "$work/landing.headers" --output "$work/landing.html" --write-out '%{http_code}' "$landing_base/")" || true
[[ "$landing_status" == 200 ]] || { echo "landing root is not public; got $landing_status" >&2; exit 1; }
grep -Eqi '^content-type:.*text/html' "$work/landing.headers" || { echo 'landing root did not return HTML' >&2; exit 1; }
grep -Eqi '<!doctype html|<html' "$work/landing.html" || { echo 'landing root response is not an HTML document' >&2; exit 1; }

app_base="${MPAKA_APP_BASE_URL:-https://app.mpaka.dev}"
app_status="$(curl --silent --show-error --proto '=https' --proto-redir '=https' --max-redirs 0 --dump-header "$work/app.headers" --output /dev/null --write-out '%{http_code}' "$app_base/")" || true
access_host="${MPAKA_ACCESS_HOST:-spluca.cloudflareaccess.com}"
require_cloudflare_access_redirect "$app_status" "$work/app.headers" "$access_host"

api_base="${MPAKA_API_BASE_URL:-https://api.mpaka.dev}"
api_status="$(curl --silent --show-error --proto '=https' --proto-redir '=https' --max-redirs 0 --dump-header "$work/api-health.headers" --output "$work/api-health.json" --write-out '%{http_code}' "$api_base/health")" || true
[[ "$api_status" == 200 ]] || { echo "API health is unavailable; got $api_status" >&2; exit 1; }

printf 'mPaka CLI public acceptance passed (compatibility %s, active %s, active manifest %s)\n' \
  "$compatibility_version" \
  "$active_version" \
  "$(sha256sum "$work/release-$active_version/manifest.json" | awk '{print $1}')"

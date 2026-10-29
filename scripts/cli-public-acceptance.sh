#!/usr/bin/env bash
set -Eeuo pipefail

base_url="${MPAKA_CLI_PUBLIC_BASE_URL:-https://cli.mpaka.dev}"
public_key="${MPAKA_CLI_PUBLIC_KEY_FILE:-}"
[[ "$base_url" == https://* ]] || { echo 'MPAKA_CLI_PUBLIC_BASE_URL must use HTTPS' >&2; exit 1; }
[[ -r "$public_key" ]] || { echo 'MPAKA_CLI_PUBLIC_KEY_FILE must point to the release public key' >&2; exit 1; }
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
grep -q '^#!/usr/bin/env bash' "$work/install"
grep -q 'param(' "$work/install.ps1"

version="$(tr -d '\r\n' < "$work/latest")"
[[ "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo "invalid latest pointer: $version" >&2; exit 1; }
get_public "$base_url/releases/$version/manifest.json" "$work/manifest.json" "$work/manifest.headers"
get_public "$base_url/releases/$version/manifest.json.sig" "$work/manifest.json.sig" "$work/manifest-sig.headers"
openssl dgst -sha256 -verify "$public_key" -signature "$work/manifest.json.sig" "$work/manifest.json" >/dev/null
[[ "$(jq -r '.version' "$work/manifest.json")" == "$version" ]] || { echo 'manifest version mismatch' >&2; exit 1; }
[[ "$(jq '.artifacts | length' "$work/manifest.json")" == 6 ]] || { echo 'manifest does not contain six artifacts' >&2; exit 1; }

while IFS=$'\t' read -r filename expected_hash; do
  get_public "$base_url/releases/$version/$filename" "$work/$filename" "$work/$filename.headers"
  [[ "$(sha256sum "$work/$filename" | awk '{print $1}')" == "$expected_hash" ]] || { echo "artifact hash mismatch: $filename" >&2; exit 1; }
done < <(jq -r '.artifacts[] | [.filename, .sha256] | @tsv' "$work/manifest.json")

landing_base="${MPAKA_LANDING_BASE_URL:-https://mpaka.dev}"
landing_status="$(curl --silent --show-error --proto '=https' --proto-redir '=https' --max-redirs 0 --dump-header "$work/landing.headers" --output /dev/null --write-out '%{http_code}' "$landing_base/")" || true
[[ "$landing_status" =~ ^3[0-9][0-9]$ ]] || { echo "landing root is not Access-protected; got $landing_status" >&2; exit 1; }
grep -Eqi 'cloudflareaccess|/cdn-cgi/access|login' "$work/landing.headers" || { echo 'landing redirect is not an Access redirect' >&2; exit 1; }

printf 'mPaka CLI public acceptance passed for %s (manifest %s)\n' "$version" "$(sha256sum "$work/manifest.json" | awk '{print $1}')"

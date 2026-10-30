#!/usr/bin/env bash

require_cloudflare_access_redirect() {
  local status="$1" headers="$2" access_host="$3" location access_prefix

  [[ "$status" =~ ^3[0-9][0-9]$ ]] || {
    printf 'expected Cloudflare Access redirect, got HTTP %s\n' "$status" >&2
    return 1
  }

  location="$(tr -d '\r' < "$headers" | awk 'tolower($0) ~ /^location:[[:space:]]*/ { sub(/^[^:]*:[[:space:]]*/, ""); print; exit }')"
  access_prefix="https://$access_host/cdn-cgi/access/login/"
  [[ "$location" == "$access_prefix"* ]] || {
    printf 'redirect location is not the expected Cloudflare Access login\n' >&2
    return 1
  }

  grep -Eqi '^www-authenticate:[[:space:]]*Cloudflare-Access([[:space:]]|$)' "$headers" || {
    printf 'Cloudflare Access authentication challenge is missing\n' >&2
    return 1
  }
}

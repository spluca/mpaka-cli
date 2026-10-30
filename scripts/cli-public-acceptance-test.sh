#!/usr/bin/env bash
set -Eeuo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$root_dir/scripts/cli-public-acceptance-lib.sh"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

cat > "$work/access.headers" <<'EOF'
HTTP/2 302
Location: https://spluca.cloudflareaccess.com/cdn-cgi/access/login/app.mpaka.dev?redirect_url=%2F
WWW-Authenticate: Cloudflare-Access resource_metadata="https://app.mpaka.dev/.well-known/cloudflare-access-protected-resource/"
EOF
require_cloudflare_access_redirect 302 "$work/access.headers" spluca.cloudflareaccess.com

cat > "$work/generic-login.headers" <<'EOF'
HTTP/2 302
location: https://app.mpaka.dev/login
EOF
if require_cloudflare_access_redirect 302 "$work/generic-login.headers" spluca.cloudflareaccess.com 2>/dev/null; then
  printf 'generic application login was accepted as Cloudflare Access\n' >&2
  exit 1
fi

cat > "$work/unrelated-text.headers" <<'EOF'
HTTP/2 302
location: https://example.com/login
x-debug: cloudflareaccess /cdn-cgi/access/login
EOF
if require_cloudflare_access_redirect 302 "$work/unrelated-text.headers" spluca.cloudflareaccess.com 2>/dev/null; then
  printf 'Access-looking text in an unrelated header was accepted\n' >&2
  exit 1
fi

cat > "$work/wrong-status.headers" <<'EOF'
HTTP/2 200
location: https://spluca.cloudflareaccess.com/cdn-cgi/access/login/app.mpaka.dev
www-authenticate: Cloudflare-Access
EOF
if require_cloudflare_access_redirect 200 "$work/wrong-status.headers" spluca.cloudflareaccess.com 2>/dev/null; then
  printf 'non-redirect status was accepted as Cloudflare Access\n' >&2
  exit 1
fi

printf 'CLI public acceptance assertion tests passed\n'

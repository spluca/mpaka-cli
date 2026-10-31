#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
subject="$root/scripts/cli-bridge-publish.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$work/bin" "$work/bundle" "$work/state/releases/v0.2.0"

cat > "$work/bin/curl" <<'SH'
#!/usr/bin/env bash
set -Eeuo pipefail
output=''
url=''
while (($#)); do
  case "$1" in
    --output) output="$2"; shift 2 ;;
    --write-out) shift 2 ;;
    --dump-header) shift 2 ;;
    --proto|--proto-redir|--max-redirs|--max-time) shift 2 ;;
    --silent|--show-error) shift ;;
    http*) url="$1"; shift ;;
    *) shift ;;
  esac
done
if [[ "${FAKE_CURL_MODE:-}" == transport ]]; then exit 7; fi
if [[ "${FAKE_CURL_MODE:-}" == server-error ]]; then printf '500'; exit 0; fi
key="${url#*://}"
key="${key#*/}"
source="$FAKE_R2_STATE/$key"
if [[ -f "$source" ]]; then
  cp "$source" "$output"
  printf '200'
else
  : > "$output"
  printf '404'
fi
SH

cat > "$work/bin/pnpm" <<'SH'
#!/usr/bin/env bash
set -Eeuo pipefail
key=''
file=''
while (($#)); do
  case "$1" in
    put) key="$2"; shift 2 ;;
    --file) file="$2"; shift 2 ;;
    *) shift ;;
  esac
done
[[ "$key" == mpaka-cli-releases/* ]]
destination="$FAKE_R2_STATE/${key#mpaka-cli-releases/}"
mkdir -p "$(dirname "$destination")"
cp "$file" "$destination"
SH
chmod 0755 "$work/bin/curl" "$work/bin/pnpm"

for target in \
  x86_64-unknown-linux-musl \
  armv7-unknown-linux-musleabihf \
  aarch64-unknown-linux-musl \
  x86_64-pc-windows-msvc \
  x86_64-apple-darwin \
  aarch64-apple-darwin; do
  extension='tar.gz'
  [[ "$target" == x86_64-pc-windows-msvc ]] && extension='zip'
  filename="mpaka-$target.$extension"
  printf 'archive:%s\n' "$target" > "$work/bundle/$filename"
done

artifacts='[]'
for archive in "$work/bundle"/mpaka-*; do
  filename="$(basename "$archive")"
  target="${filename#mpaka-}"
  target="${target%.tar.gz}"
  target="${target%.zip}"
  size="$(wc -c < "$archive" | tr -d '[:space:]')"
  digest="$(sha256sum "$archive" | awk '{print $1}')"
  artifacts="$(jq -c --arg target "$target" --arg filename "$filename" --argjson size "$size" --arg sha256 "$digest" '. + [{target:$target,filename:$filename,size:$size,sha256:$sha256}]' <<<"$artifacts")"
done
jq -c -S -n --arg version v0.2.0 --argjson artifacts "$artifacts" '{version:$version,artifacts:($artifacts|sort_by(.target))}' > "$work/bundle/manifest.json"
printf 'detached-signature' > "$work/bundle/manifest.json.sig"
printf '#!/usr/bin/env bash\n' > "$work/bundle/install"
printf 'param()\n' > "$work/bundle/install.ps1"

run_subject() {
  PATH="$work/bin:$PATH" FAKE_R2_STATE="$work/state" "$subject" "$@"
}

run_subject release --bundle "$work/bundle" --base-url https://cli.example
for file in "$work/bundle"/manifest.json "$work/bundle"/manifest.json.sig "$work/bundle"/mpaka-*; do
  cmp "$file" "$work/state/releases/v0.2.0/$(basename "$file")"
done

rm "$work/state/releases/v0.2.0/mpaka-aarch64-unknown-linux-musl.tar.gz"
run_subject release --bundle "$work/bundle" --base-url https://cli.example
cmp "$work/bundle/mpaka-aarch64-unknown-linux-musl.tar.gz" "$work/state/releases/v0.2.0/mpaka-aarch64-unknown-linux-musl.tar.gz"

printf 'conflict' > "$work/state/releases/v0.2.0/manifest.json.sig"
if run_subject release --bundle "$work/bundle" --base-url https://cli.example >/dev/null 2>&1; then
  printf 'conflicting immutable object was overwritten\n' >&2
  exit 1
fi
test "$(cat "$work/state/releases/v0.2.0/manifest.json.sig")" = conflict
cp "$work/bundle/manifest.json.sig" "$work/state/releases/v0.2.0/manifest.json.sig"

rm "$work/state/releases/v0.2.0/mpaka-x86_64-apple-darwin.tar.gz"
if PATH="$work/bin:$PATH" FAKE_R2_STATE="$work/state" FAKE_CURL_MODE=server-error "$subject" release --bundle "$work/bundle" --base-url https://cli.example >/dev/null 2>&1; then
  printf 'HTTP 500 was treated as confirmed absence\n' >&2
  exit 1
fi
test ! -e "$work/state/releases/v0.2.0/mpaka-x86_64-apple-darwin.tar.gz"

if PATH="$work/bin:$PATH" FAKE_R2_STATE="$work/state" FAKE_CURL_MODE=transport "$subject" release --bundle "$work/bundle" --base-url https://cli.example >/dev/null 2>&1; then
  printf 'transport failure was treated as confirmed absence\n' >&2
  exit 1
fi

run_subject installers --bundle "$work/bundle" --base-url https://cli.example
cmp "$work/bundle/install" "$work/state/install"
cmp "$work/bundle/install.ps1" "$work/state/install.ps1"

printf 'bridge publication reconciliation tests passed\n'

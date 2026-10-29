#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

input="$work/input"
output="$work/output"
release="$work/release"
mkdir -p "$input" "$output" "$release"

targets=(
  x86_64-unknown-linux-musl
  armv7-unknown-linux-musleabihf
  aarch64-unknown-linux-musl
  x86_64-pc-windows-msvc
  x86_64-apple-darwin
  aarch64-apple-darwin
)
for target in "${targets[@]}"; do
  if [[ "$target" == x86_64-pc-windows-msvc ]]; then
    printf 'fixture %s\r\n' "$target" > "$input/mpaka.exe"
  else
    printf 'fixture %s\n' "$target" > "$input/mpaka"
  fi
  cp "$input/$( [[ "$target" == x86_64-pc-windows-msvc ]] && printf 'mpaka.exe' || printf 'mpaka' )" "$input/mpaka-$target"
  rm "$input/mpaka" "$input/mpaka.exe" 2>/dev/null || true
done

key="$work/signing-key.pem"
openssl genrsa 2048 2>/dev/null > "$key"
openssl rsa -in "$key" -pubout -out "$work/public.pem" 2>/dev/null

"$root/scripts/cli-release.sh" package \
  --version v0.1.0 \
  --input-dir "$input" \
  --output-dir "$output"
"$root/scripts/cli-release.sh" manifest \
  --version v0.1.0 \
  --artifact-dir "$output" \
  --output-dir "$release" \
  --signing-key "$key"
"$root/scripts/cli-release.sh" verify \
  --version v0.1.0 \
  --release-dir "$release" \
  --public-key "$work/public.pem"

test "$(jq -r '.version' "$release/manifest.json")" = v0.1.0
test "$(jq '.artifacts | length' "$release/manifest.json")" = 6
test "$(jq -r '.artifacts | sort_by(.target) | .[0].sha256' "$release/manifest.json" | wc -c)" -eq 65

first_hash="$(sha256sum "$release/manifest.json")"
rm "$release/manifest.json" "$release/manifest.json.sig"
"$root/scripts/cli-release.sh" manifest \
  --version v0.1.0 \
  --artifact-dir "$output" \
  --output-dir "$release" \
  --signing-key "$key"
test "$first_hash" = "$(sha256sum "$release/manifest.json")"

bad_artifacts="$work/bad-artifacts"
bad_release="$work/bad-release"
bad_stage="$work/bad-stage"
mkdir -p "$bad_artifacts" "$bad_release" "$bad_stage"
cp "$output"/* "$bad_artifacts/"
cp "$input/mpaka-x86_64-unknown-linux-musl" "$bad_stage/mpaka"
printf 'escape\n' > "$work/escape"
tar -cf "$bad_artifacts/mpaka-x86_64-unknown-linux-musl.tar" -C "$bad_stage" mpaka
tar --append --file "$bad_artifacts/mpaka-x86_64-unknown-linux-musl.tar" -C "$work" escape
gzip -n -f "$bad_artifacts/mpaka-x86_64-unknown-linux-musl.tar"
"$root/scripts/cli-release.sh" manifest \
  --version v0.1.0 \
  --artifact-dir "$bad_artifacts" \
  --output-dir "$bad_release" \
  --signing-key "$key"
if "$root/scripts/cli-release.sh" verify \
  --version v0.1.0 \
  --release-dir "$bad_release" \
  --public-key "$work/public.pem"; then
  printf 'unsafe archive was accepted\n' >&2
  exit 1
fi

printf 'cli release fixture tests passed\n'

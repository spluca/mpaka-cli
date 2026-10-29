#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_NAME="$(basename "$0")"
readonly SCRIPT_NAME
readonly TARGETS=(
  x86_64-unknown-linux-musl
  armv7-unknown-linux-musleabihf
  aarch64-unknown-linux-musl
  x86_64-pc-windows-msvc
  x86_64-apple-darwin
  aarch64-apple-darwin
)

die() {
  printf '%s: %s\n' "$SCRIPT_NAME" "$*" >&2
  exit 1
}

usage() {
  cat >&2 <<'EOF'
usage:
  scripts/cli-release.sh package --version vX.Y.Z --input-dir DIR --output-dir DIR
  scripts/cli-release.sh manifest --version vX.Y.Z --artifact-dir DIR --output-dir DIR --signing-key FILE
  scripts/cli-release.sh verify --version vX.Y.Z --release-dir DIR --public-key FILE
EOF
  exit 2
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || die "required command is unavailable: $1"
}

parse_version() {
  [[ "$1" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "version must match vMAJOR.MINOR.PATCH: $1"
}

target_archive() {
  case "$1" in
    x86_64-unknown-linux-musl) printf '%s' 'mpaka-x86_64-unknown-linux-musl.tar.gz' ;;
    armv7-unknown-linux-musleabihf) printf '%s' 'mpaka-armv7-unknown-linux-musleabihf.tar.gz' ;;
    aarch64-unknown-linux-musl) printf '%s' 'mpaka-aarch64-unknown-linux-musl.tar.gz' ;;
    x86_64-pc-windows-msvc) printf '%s' 'mpaka-x86_64-pc-windows-msvc.zip' ;;
    x86_64-apple-darwin) printf '%s' 'mpaka-x86_64-apple-darwin.tar.gz' ;;
    aarch64-apple-darwin) printf '%s' 'mpaka-aarch64-apple-darwin.tar.gz' ;;
    *) die "unsupported release target: $1" ;;
  esac
}

target_executable() {
  if [[ "$1" == x86_64-pc-windows-msvc ]]; then
    printf '%s' 'mpaka.exe'
  else
    printf '%s' 'mpaka'
  fi
}

hash_file() {
  sha256sum "$1" | awk '{print $1}'
}

size_file() {
  wc -c < "$1" | tr -d '[:space:]'
}

package_target() {
  local version="$1" input_dir="$2" output_dir="$3" target="$4"
  local source archive executable staging
  source="$input_dir/mpaka-$target"
  archive="$(target_archive "$target")"
  executable="$(target_executable "$target")"
  staging="$output_dir/.staging-$target"
  [[ -f "$source" ]] || die "missing prebuilt binary: $source"
  rm -rf "$staging"
  mkdir -p "$staging"
  cp "$source" "$staging/$executable"
  chmod 0755 "$staging/$executable"
  if [[ "$archive" == *.zip ]]; then
    (cd "$staging" && touch -d '@0' "$executable" && zip -X -q "$output_dir/$archive" "$executable")
  else
    local tar_file="$output_dir/${archive%.gz}"
    (cd "$staging" && touch -d '@0' "$executable" && tar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner -cf "$tar_file" "$executable")
    gzip -n -f "$tar_file"
  fi
  rm -rf "$staging"
  [[ -s "$output_dir/$archive" ]] || die "failed to create archive: $archive"
  : "$version"
}

package_release() {
  local version="$1" input_dir="$2" output_dir="$3"
  require_command tar
  require_command gzip
  require_command zip
  mkdir -p "$output_dir"
  rm -f "$output_dir"/*
  local target
  for target in "${TARGETS[@]}"; do
    package_target "$version" "$input_dir" "$output_dir" "$target"
  done
}

write_manifest() {
  local version="$1" artifact_dir="$2" output_dir="$3" signing_key="$4"
  require_command jq
  require_command openssl
  require_command sha256sum
  [[ -r "$signing_key" ]] || die "signing key is not readable: $signing_key"
  mkdir -p "$output_dir"
  local target archive size digest
  local artifacts='[]'
  for target in "${TARGETS[@]}"; do
    archive="$(target_archive "$target")"
    [[ -f "$artifact_dir/$archive" ]] || die "missing release archive: $artifact_dir/$archive"
    size="$(size_file "$artifact_dir/$archive")"
    digest="$(hash_file "$artifact_dir/$archive")"
    artifacts="$(jq -c --arg target "$target" --arg filename "$archive" --argjson size "$size" --arg sha256 "$digest" '. + [{target: $target, filename: $filename, size: $size, sha256: $sha256}]' <<<"$artifacts")"
    cp "$artifact_dir/$archive" "$output_dir/$archive"
  done
  jq -c -S -n --arg version "$version" --argjson artifacts "$artifacts" '{version: $version, artifacts: ($artifacts | sort_by(.target))}' > "$output_dir/manifest.json"
  openssl dgst -sha256 -sign "$signing_key" -out "$output_dir/manifest.json.sig" "$output_dir/manifest.json"
  chmod 0644 "$output_dir/manifest.json" "$output_dir/manifest.json.sig" "$output_dir"/*
}

validate_archive() {
  local archive="$1" executable="$2" listing
  if [[ "$archive" == *.zip ]]; then
    require_command unzip
    listing="$(unzip -Z1 "$archive")"
  else
    listing="$(tar -tzf "$archive")"
  fi
  [[ "$listing" == "$executable" ]] || die "archive contains unexpected entries: $archive"
}

verify_release() {
  local version="$1" release_dir="$2" public_key="$3"
  require_command jq
  require_command openssl
  require_command sha256sum
  [[ -r "$release_dir/manifest.json" ]] || die "manifest is missing"
  [[ -r "$release_dir/manifest.json.sig" ]] || die "manifest signature is missing"
  [[ -r "$public_key" ]] || die "public key is not readable: $public_key"
  openssl dgst -sha256 -verify "$public_key" -signature "$release_dir/manifest.json.sig" "$release_dir/manifest.json" >/dev/null || die "manifest signature verification failed"
  [[ "$(jq -r '.version' "$release_dir/manifest.json")" == "$version" ]] || die "manifest version does not match $version"
  [[ "$(jq '.artifacts | length' "$release_dir/manifest.json")" == 6 ]] || die "manifest must contain six artifacts"
  local target archive executable expected_size expected_hash actual_size
  while IFS=$'\t' read -r target archive expected_size expected_hash; do
    [[ -n "$target" ]] || continue
    [[ "$archive" == "$(target_archive "$target")" ]] || die "unexpected archive for target $target"
    [[ -f "$release_dir/$archive" ]] || die "manifest archive is missing: $archive"
    actual_size="$(size_file "$release_dir/$archive")"
    [[ "$actual_size" == "$expected_size" ]] || die "size mismatch for $archive"
    [[ "$(hash_file "$release_dir/$archive")" == "$expected_hash" ]] || die "SHA-256 mismatch for $archive"
    executable="$(target_executable "$target")"
    validate_archive "$release_dir/$archive" "$executable"
  done < <(jq -r '.artifacts[] | [.target, .filename, (.size | tostring), .sha256] | @tsv' "$release_dir/manifest.json")
}

command_name="${1:-}"
shift || true
version=''
input_dir=''
artifact_dir=''
output_dir=''
release_dir=''
signing_key=''
public_key=''
while (($#)); do
  case "$1" in
    --version) version="${2:-}"; shift 2 ;;
    --input-dir) input_dir="${2:-}"; shift 2 ;;
    --artifact-dir) artifact_dir="${2:-}"; shift 2 ;;
    --output-dir) output_dir="${2:-}"; shift 2 ;;
    --release-dir) release_dir="${2:-}"; shift 2 ;;
    --signing-key) signing_key="${2:-}"; shift 2 ;;
    --public-key) public_key="${2:-}"; shift 2 ;;
    *) usage ;;
  esac
done
parse_version "$version"
case "$command_name" in
  package) [[ -n "$input_dir" && -n "$output_dir" ]] || usage; package_release "$version" "$input_dir" "$output_dir" ;;
  manifest) [[ -n "$artifact_dir" && -n "$output_dir" && -n "$signing_key" ]] || usage; write_manifest "$version" "$artifact_dir" "$output_dir" "$signing_key" ;;
  verify) [[ -n "$release_dir" && -n "$public_key" ]] || usage; verify_release "$version" "$release_dir" "$public_key" ;;
  *) usage ;;
esac

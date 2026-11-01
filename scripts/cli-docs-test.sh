#!/usr/bin/env bash
set -Eeuo pipefail

root_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
docs_file="$root_dir/docs/cli.md"
release_file="$root_dir/docs/release.md"

required_snippets=(
  'curl -fsSL https://cli.mpaka.dev/install | bash'
  'bash -s -- --version v0.3.0'
  'mpaka self-update --version v0.3.0'
  'chmod +x ~/.local/bin/mpaka'
  'https://cli.mpaka.dev/install.ps1'
  'SHA-256'
  'x86_64-unknown-linux-musl'
  'armv7-unknown-linux-musleabihf'
  'aarch64-unknown-linux-musl'
  'x86_64-pc-windows-msvc'
  'x86_64-apple-darwin'
  'aarch64-apple-darwin'
  'mpaka self-update --check'
  'latest'
  'latest-v2'
  'run `mpaka self-update` twice'
  'rollback'
)

for snippet in "${required_snippets[@]}"; do
  if ! grep -Fq -- "$snippet" "$docs_file"; then
    printf 'missing CLI documentation snippet: %s\n' "$snippet" >&2
    exit 1
  fi
done

for snippet in \
  '`latest` remains fixed at `v0.2.0`' \
  '`latest-v2` is the active stable pointer' \
  'MPAKA_CLI_LEGACY_PUBLIC_KEY_FILE' \
  'MPAKA_CLI_ACTIVE_PUBLIC_KEY_FILE'; do
  if ! grep -Fq -- "$snippet" "$release_file"; then
    printf 'missing release documentation snippet: %s\n' "$snippet" >&2
    exit 1
  fi
done

# Current clients and installers reject manifests signed by the retired key.
if grep -Eq -- '--version v0\.[12]\.0' "$docs_file"; then
  printf 'CLI documentation pins a release signed by the retired key\n' >&2
  exit 1
fi

if grep -Fq 'curl -fsSL https://cli.mpaka.dev/install.ps1 | bash' "$docs_file"; then
  printf 'PowerShell installer documented with the wrong shell\n' >&2
  exit 1
fi

printf 'CLI documentation checks passed\n'

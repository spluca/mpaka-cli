#!/usr/bin/env bash
set -Eeuo pipefail

root_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
docs_file="$root_dir/docs/cli.md"

required_snippets=(
  'curl -fsSL https://cli.mpaka.dev/install | bash'
  '--version v0.1.0'
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
  'rollback'
)

for snippet in "${required_snippets[@]}"; do
  if ! grep -Fq -- "$snippet" "$docs_file"; then
    printf 'missing CLI documentation snippet: %s\n' "$snippet" >&2
    exit 1
  fi
done

if grep -Fq 'curl -fsSL https://cli.mpaka.dev/install.ps1 | bash' "$docs_file"; then
  printf 'PowerShell installer documented with the wrong shell\n' >&2
  exit 1
fi

printf 'CLI documentation checks passed\n'

#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
script="$root/install.ps1"
test -s "$script"
grep -Fq 'Get-TargetTriple' "$script"
grep -Fq 'Test-SafeZipEntries' "$script"
grep -Fq 'VerifyData' "$script"
grep -Fq 'Get-FileHash' "$script"
grep -Fq 'Invoke-WebRequest' "$script" || grep -Fq 'HttpClient' "$script"
grep -Fq 'Move-Item' "$script"
grep -Fq 'LOCALAPPDATA' "$script"
grep -Fq 'https://' "$script"
grep -Fq 'MPAKA_CLI_TEST_PUBLIC_KEY_PEM' "$script"
grep -Fq 'ReleasePublicKeyPem' "$script"
if grep -Eq 'OldReleasePublicKeyPem|NewReleasePublicKeyPem|MPAKA_CLI_TEST_FALLBACK_PUBLIC_KEY_PEM' "$script"; then
  printf 'PowerShell installer still carries bridge-era key trust\n' >&2
  exit 1
fi
test "$(grep -c '^-----BEGIN PUBLIC KEY-----$' "$script")" -eq 1
retired_body="$(grep -v 'PUBLIC KEY' "$root/tests/fixtures/retired-key/public.pem" | tr -d '\n')"
if tr -d '\r\n' < "$script" | grep -Fq "$retired_body"; then
  printf 'PowerShell installer still embeds the retired release key\n' >&2
  exit 1
fi
grep -Fq 'LatestPointerPath' "$script"
grep -Fq 'latest-v2' "$script"
printf 'PowerShell installer static checks passed\n'

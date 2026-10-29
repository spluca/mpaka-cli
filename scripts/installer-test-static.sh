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
printf 'PowerShell installer static checks passed\n'

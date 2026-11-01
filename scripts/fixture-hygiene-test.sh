#!/usr/bin/env bash
set -Eeuo pipefail

# `.gitignore` excludes *.pem but re-admits tests/fixtures/**/public.pem so the
# retired release key's public half can be committed. Prove no fixture ever
# carries private key material through that exception.
root="${FIXTURE_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
fixtures="$root/tests/fixtures"
test -d "$fixtures"

while IFS= read -r -d '' file; do
  if grep -lq 'PRIVATE KEY' "$file"; then
    printf 'fixture contains private key material: %s\n' "${file#"$root/"}" >&2
    exit 1
  fi
done < <(find "$fixtures" -type f -print0)

while IFS= read -r -d '' file; do
  case "$(basename "$file")" in
    public.pem) ;;
    *) printf 'only public.pem may be committed under tests/fixtures: %s\n' "${file#"$root/"}" >&2; exit 1 ;;
  esac
done < <(find "$fixtures" -type f -name '*.pem' -print0)

# Signature fixtures must check out byte-for-byte on every platform. Git for
# Windows converts text files to CRLF by default, which changes the signed
# bytes and makes a valid signature fail to verify.
if git -C "$root" rev-parse --git-dir >/dev/null 2>&1; then
  while IFS= read -r -d '' file; do
    relative="${file#"$root/"}"
    if [[ "$(git -C "$root" check-attr text -- "$relative" | sed 's/.*: //')" != unset ]]; then
      printf 'fixture is subject to end-of-line conversion: %s\n' "$relative" >&2
      exit 1
    fi
  done < <(find "$fixtures" -type f -print0)
fi

printf 'fixture hygiene checks passed\n'

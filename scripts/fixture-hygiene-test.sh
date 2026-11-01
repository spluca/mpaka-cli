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

printf 'fixture hygiene checks passed\n'

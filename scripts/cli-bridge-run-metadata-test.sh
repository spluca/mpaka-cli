#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
subject="$root/scripts/cli-bridge-run-metadata.sh"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

expect_failure() {
  if "$@" >/dev/null 2>&1; then
    printf 'expected metadata validation to fail\n' >&2
    exit 1
  fi
}

commit='0123456789abcdef0123456789abcdef01234567'
cat > "$work/run.json" <<JSON
{
  "id": 12345,
  "status": "completed",
  "conclusion": "success",
  "event": "workflow_dispatch",
  "head_sha": "$commit",
  "path": ".github/workflows/release.yml",
  "repository": {"full_name": "spluca/mpaka-cli"},
  "head_repository": {"full_name": "spluca/mpaka-cli"}
}
JSON

"$subject" validate --metadata "$work/run.json" --repository spluca/mpaka-cli --run-id 12345 --commit "$commit"

for mutation in \
  '.conclusion = "failure"' \
  '.event = "push"' \
  '.head_sha = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"' \
  '.path = ".github/workflows/other.yml"' \
  '.repository.full_name = "attacker/repo"' \
  '.head_repository.full_name = "attacker/repo"' \
  '.id = 99999'; do
  jq "$mutation" "$work/run.json" > "$work/bad.json"
  expect_failure "$subject" validate --metadata "$work/bad.json" --repository spluca/mpaka-cli --run-id 12345 --commit "$commit"
done

printf 'bridge run metadata tests passed\n'

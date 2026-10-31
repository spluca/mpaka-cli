#!/usr/bin/env bash
set -Eeuo pipefail

die() {
  printf 'cli-bridge-run-metadata: %s\n' "$*" >&2
  exit 1
}

[[ "${1:-}" == validate ]] || die 'usage: validate --metadata FILE --repository OWNER/REPO --run-id ID --commit SHA'
shift

metadata=''
repository=''
run_id=''
commit=''
while (($#)); do
  case "$1" in
    --metadata) metadata="${2:-}"; shift 2 ;;
    --repository) repository="${2:-}"; shift 2 ;;
    --run-id) run_id="${2:-}"; shift 2 ;;
    --commit) commit="${2:-}"; shift 2 ;;
    *) die "unexpected argument: $1" ;;
  esac
done

[[ -r "$metadata" && -f "$metadata" ]] || die 'metadata is not a readable regular file'
[[ "$repository" =~ ^[^/]+/[^/]+$ ]] || die 'repository must be OWNER/REPO'
[[ "$run_id" =~ ^[1-9][0-9]*$ ]] || die 'run ID must be a positive integer'
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || die 'commit must be a lowercase forty-character SHA'

jq -e \
  --arg repository "$repository" \
  --arg commit "$commit" \
  --argjson run_id "$run_id" '
    .id == $run_id and
    .status == "completed" and
    .conclusion == "success" and
    .event == "workflow_dispatch" and
    .head_sha == $commit and
    .path == ".github/workflows/release.yml" and
    .repository.full_name == $repository and
    .head_repository.full_name == $repository
  ' "$metadata" >/dev/null || die 'preparation run metadata does not match the approved bridge source'

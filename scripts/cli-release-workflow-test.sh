#!/usr/bin/env bash
set -Eeuo pipefail

workflow="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.github/workflows/release.yml"
bridge_workflow="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/.github/workflows/bridge-publish.yml"
test -s "$workflow"
test -s "$bridge_workflow"
grep -Fq 'pull_request' "$workflow"
grep -Fq 'tags:' "$workflow"
grep -Fq -- "- 'v*'" "$workflow"
grep -Fq 'contents: read' "$workflow"
grep -Fq 'CLI_RELEASE_SIGNING_KEY' "$workflow"
grep -Fq 'github.event_name == '\''push'\''' "$workflow"
grep -Fq 'github.ref_type == '\''tag'\''' "$workflow"
grep -Fq 'mpaka-cli-releases' "$workflow"
grep -Fq 'latest' "$workflow"
grep -Fq 'manifest.json.sig' "$workflow"
grep -Fq 'cli.mpaka.dev' "$workflow"
grep -Fq '[self-hosted, Linux, X64, rustbox-cli]' "$workflow"
grep -Fq 'cargo xwin build' "$workflow"
grep -Fq 'cargo zigbuild' "$workflow"
grep -Fq 'cross build' "$workflow"
grep -Fq "grep -Fq 'mPaka CLI'" "$workflow"
grep -Fq "environment: \${{ github.event_name == 'push' && github.ref_type == 'tag' && github.ref_name != 'v0.2.0' && 'release' || 'verify' }}" "$workflow"
grep -Fq "github.ref_name != 'v0.2.0' && secrets.MPAKA_CLI_RELEASE_SIGNING_KEY || ''" "$workflow"
grep -Fq 'powershell-installer:' "$workflow"
grep -Fq 'runs-on: windows-2025' "$workflow"
grep -Fq 'pwsh -NoProfile -File scripts/installer-test.ps1' "$workflow"
grep -Fq 'needs: [build, powershell-installer]' "$workflow"
grep -Fq 'workflow_dispatch:' "$workflow"
grep -Fq 'bridge_version:' "$workflow"
grep -Fq -- "- 'v0.2.0'" "$workflow"
grep -Fq 'mpaka-cli-v0.2.0-prepared' "$workflow"
grep -Fq 'source-commit' "$workflow"
grep -Fq 'manifest.sha256' "$workflow"
grep -Fq 'rm -f "$work/release/manifest.json.sig"' "$workflow"
grep -Fq "github.ref_name != 'v0.2.0'" "$workflow"
grep -Fq 'mpaka-cli-releases/latest-v2' "$workflow"

for required in \
  'actions: read' \
  'contents: read' \
  'environment: release' \
  "github.actor == github.repository_owner" \
  'prepare_run_id:' \
  'expected_commit:' \
  'manifest_sha256:' \
  'manifest_signature_base64:' \
  'run-id: ${{ inputs.prepare_run_id }}' \
  'mpaka-cli-v0.2.0-prepared' \
  'immutable bridge release already exists with a different manifest' \
  'openssl dgst -sha256 -verify' \
  'scripts/cli-release.sh verify --version v0.2.0' \
  'mpaka-cli-releases/latest-v2' \
  'mpaka-cli-releases/latest'; do
  grep -Fq -- "$required" "$bridge_workflow"
done
if grep -Fq 'MPAKA_CLI_RELEASE_SIGNING_KEY' "$bridge_workflow"; then
  printf 'bridge publisher must not access a private signing key\n' >&2
  exit 1
fi
comparison_line="$(grep -nF 'cmp --silent "$file"' "$bridge_workflow" | tail -1 | cut -d: -f1)"
latest_line="$(grep -nF 'mpaka-cli-releases/latest"' "$bridge_workflow" | tail -1 | cut -d: -f1)"
latest_v2_line="$(grep -nF 'mpaka-cli-releases/latest-v2"' "$bridge_workflow" | tail -1 | cut -d: -f1)"
test "$latest_line" -gt "$comparison_line"
test "$latest_v2_line" -gt "$comparison_line"
unpinned_actions="$(grep -E '^[[:space:]]+(-[[:space:]]+)?uses:' "$workflow" "$bridge_workflow" | grep -Ev 'uses:[[:space:]]+[^[:space:]]+@[0-9a-f]{40}([[:space:]]+#[[:space:]].+)?$' || true)"
if [[ -n "$unpinned_actions" ]]; then
  printf 'every third-party Action must be pinned to a commit SHA:\n%s\n' "$unpinned_actions" >&2
  exit 1
fi
# This repository is public: self-hosted jobs must never run fork pull requests.
fork_guard="if: github.event_name != 'pull_request' || github.event.pull_request.head.repo.full_name == github.repository"
runner_jobs="$(grep -c 'runs-on: \[self-hosted' "$workflow")"
guarded_jobs="$(grep -cF "$fork_guard" "$workflow")"
if [[ "$runner_jobs" -ne "$guarded_jobs" ]]; then
  printf 'every self-hosted job must skip fork pull requests\n' >&2
  exit 1
fi
for target in \
  x86_64-unknown-linux-musl \
  armv7-unknown-linux-musleabihf \
  aarch64-unknown-linux-musl \
  x86_64-pc-windows-msvc \
  x86_64-apple-darwin \
  aarch64-apple-darwin; do
  grep -Fq "$target" "$workflow"
done
if grep -Eq 'run:.*(wrangler|r2 object put)' "$workflow" && ! grep -Fq "github.event_name == 'push'" "$workflow"; then
  printf 'publication command is not tag-gated\n' >&2
  exit 1
fi
printf 'cli release workflow static checks passed\n'

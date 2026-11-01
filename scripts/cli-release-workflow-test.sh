#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workflow="$root/.github/workflows/release.yml"
test -s "$workflow"
grep -Fq 'pull_request' "$workflow"
grep -Fq 'tags:' "$workflow"
grep -Fq -- "- 'v*'" "$workflow"
grep -Fq 'contents: read' "$workflow"
grep -Fq 'CLI_RELEASE_SIGNING_KEY' "$workflow"
grep -Fq 'github.event_name == '\''push'\''' "$workflow"
grep -Fq 'github.ref_type == '\''tag'\''' "$workflow"
grep -Fq 'mpaka-cli-releases' "$workflow"
grep -Fq 'manifest.json.sig' "$workflow"
grep -Fq 'cli.mpaka.dev' "$workflow"
grep -Fq '[self-hosted, Linux, X64, rustbox-cli]' "$workflow"
grep -Fq 'cargo xwin build' "$workflow"
grep -Fq 'cargo zigbuild' "$workflow"
grep -Fq 'cross build' "$workflow"
grep -Fq "grep -Fq 'mPaka CLI'" "$workflow"
grep -Fq "environment: \${{ github.event_name == 'push' && github.ref_type == 'tag' && 'release' || 'verify' }}" "$workflow"
grep -Fq "github.event_name == 'push' && github.ref_type == 'tag' && secrets.MPAKA_CLI_RELEASE_SIGNING_KEY || ''" "$workflow"
grep -Fq 'powershell-installer:' "$workflow"
grep -Fq 'runs-on: windows-2025' "$workflow"
grep -Fq 'pwsh -NoProfile -File scripts/installer-test.ps1' "$workflow"
grep -Fq 'needs: [build, powershell-installer]' "$workflow"

# Production tags must sign with the key that clients and installers embed.
grep -Fq 'release signing key does not match the embedded public key' "$workflow"
# The signing key must not survive a failure between writing it and signing.
grep -Fq "trap 'rm -f \"\$work/signing-key.pem\"' EXIT" "$workflow"

# A tag must never move latest-v2 backward, and never to a retired-key release.
grep -Fq 'refusing to move latest-v2 backward' "$workflow"
grep -Fq 'MINIMUM_ACTIVE_VERSION' "$workflow"

# The v0.2.0 bridge is complete: no preparation path, bridge publisher, or helper remains.
test ! -e "$root/.github/workflows/bridge-publish.yml"
for retired in cli-bridge-publish.sh cli-bridge-publish-test.sh cli-bridge-run-metadata.sh cli-bridge-run-metadata-test.sh; do
  test ! -e "$root/scripts/$retired"
done
if grep -Eq 'workflow_dispatch|bridge_version|v0\.2\.0|prepared' "$workflow"; then
  printf 'release workflow still carries v0.2.0 bridge preparation\n' >&2
  exit 1
fi

# Releases advance only latest-v2; the legacy latest pointer stays on the v0.2.0 bridge.
grep -Fq 'mpaka-cli-releases/latest-v2"' "$workflow"
if grep -Fq 'mpaka-cli-releases/latest"' "$workflow"; then
  printf 'release workflow must never move the legacy latest pointer\n' >&2
  exit 1
fi
verify_line="$(grep -nF 'Verify public release before latest-v2 pointer' "$workflow" | cut -d: -f1)"
pointer_line="$(grep -nF 'mpaka-cli-releases/latest-v2"' "$workflow" | cut -d: -f1)"
test "$pointer_line" -gt "$verify_line"

# The public installers trust only the active key, so they must not be published
# before latest-v2 names the release they can verify.
installer_line="$(grep -nF 'mpaka-cli-releases/$installer"' "$workflow" | cut -d: -f1)"
test "$installer_line" -gt "$pointer_line"

unpinned_actions="$(grep -E '^[[:space:]]+(-[[:space:]]+)?uses:' "$workflow" | grep -Ev 'uses:[[:space:]]+[^[:space:]]+@[0-9a-f]{40}([[:space:]]+#[[:space:]].+)?$' || true)"
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

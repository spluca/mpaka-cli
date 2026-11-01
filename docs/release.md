# mPaka CLI release operations

The CLI distribution is served at `https://cli.mpaka.dev`. Versioned release
objects are immutable. `latest` remains fixed at `v0.2.0` as the compatibility
bridge for v0.1.0 clients, while `latest-v2` is the active stable pointer for
v0.2.0 and every later client and installer.

## Cloudflare configuration

Create the R2 bucket `mpaka-cli-releases` and deploy the Worker in
`cli-distribution/` with the `CLI_RELEASES` binding. The Worker route is
`cli.mpaka.dev/*`. It must be deployed separately from the Astro landing
Worker so release history does not grow the landing asset bundle.

The landing at `mpaka.dev/*` is public. Keep the existing Cloudflare Access
application protecting `app.mpaka.dev/*`, and add a public Access application
for exactly:

```text
cli.mpaka.dev/*
```

The CLI application may use Bypass for public downloads, but it must not cover
the console or the API. Verify anonymously that the CLI downloads and
`https://mpaka.dev/` return 200, `https://app.mpaka.dev/` redirects to the
existing Access login, and `https://api.mpaka.dev/health` returns 200. Run
`scripts/cli-public-acceptance.sh` after every Access or Worker change.

## GitHub configuration

The release build and publication pipeline runs on a dedicated self-hosted
Linux runner labelled `rustbox-cli`. Linux targets use `cross`, the Windows
MSVC target uses `cargo-xwin` and Clang, and macOS targets use
`cargo-zigbuild` with the pinned Zig setup action. Foreign binaries are
validated with `file`. A separate GitHub-hosted Windows job executes the
PowerShell installer fixture tests with real PowerShell; it does not receive
release secrets or publish artifacts.

Because this repository is public, every self-hosted job skips pull requests
whose head branch lives in a fork, and
`scripts/cli-release-workflow-test.sh` fails if a self-hosted job loses that
guard. Also enable *Require approval for all external contributors* in the
repository Actions settings. External contributions are verified by a
maintainer pushing the branch to this repository.

Create a `release` environment restricted to `v*` tags and store the values
below there, not as repository-wide secrets. Pull requests run the package job
in the `verify` environment, which holds no secrets. The release workflow uses
these protected values:

- Secret `MPAKA_CLI_RELEASE_SIGNING_KEY`: PEM private RSA-3072 key generated
  outside Git and never printed or committed. Its public half must be the
  single `RELEASE_PUBLIC_KEY_PEM` embedded in `src/distribution.rs`, `install`,
  and `install.ps1` (SPKI SHA-256
  `0f0d2394b0c44b13adaa78bb4b913fc989813fe6c2fa85fdd9f6e20bd043b9fc`). Tag
  builds fail before publishing if the secret does not match.
- Secret `MPAKA_CLI_CLOUDFLARE_API_TOKEN`: scoped to deploy the distribution
  Worker and write the `mpaka-cli-releases` bucket.
- Variable `MPAKA_CLI_CLOUDFLARE_ACCOUNT_ID`: the Cloudflare account ID.

Pull requests generate a temporary test key and never publish. A production
publication occurs only for an annotated `vMAJOR.MINOR.PATCH` tag matching the
workspace version. The `release` environment admits only `v*` tag refs, so any
workflow that needs those secrets must run from a tag; a run from a branch is
rejected before it starts.

## Release and rollback

The tag workflow builds and smoke-tests all six targets, signs the exact
`manifest.json` bytes, publishes `releases/<version>/`, verifies every public
object, writes `latest-v2`, and publishes `install` and `install.ps1` last. The
installers trust only the active key, so they must never be published before
`latest-v2` names a release they can verify. The workflow also refuses any tag
older than `v0.3.0` or older than the current public `latest-v2`, so a re-run of
a superseded tag cannot roll the pointer backward. It must never advance
`latest` beyond v0.2.0.

To roll back, choose an already published version from v0.3.0 onward and
update only the R2 `latest-v2` object to that exact `vX.Y.Z` line. Never point
`latest-v2` at v0.2.0 or earlier: those manifests are signed by the retired
key, which v0.3.0 and later clients reject. Do not change `latest`, and do not
overwrite or delete the version prefix. Then run:

```bash
MPAKA_CLI_LEGACY_PUBLIC_KEY_FILE=/secure/path/mpaka-old-release-public.pem \
MPAKA_CLI_ACTIVE_PUBLIC_KEY_FILE=/secure/path/mpaka-release-public.pem \
MPAKA_CLI_PUBLIC_BASE_URL=https://cli.mpaka.dev \
  scripts/cli-public-acceptance.sh
```

If verification fails, restore the previous `latest-v2` value and investigate
the Worker/R2 or Access change before attempting another rollback.

## Key rotation

The original release key was retired in two steps:

1. The v0.2.0 bridge trusts the old and new RSA-3072 public keys but is signed
   by the old key, so v0.1.0 can install it. It is published once, and
   `latest` stays on v0.2.0 permanently.
2. v0.3.0 and later trust only the new key (fingerprint above), are signed with
   it by the normal tag pipeline, and advance only `latest-v2`.

A v0.1.0 client therefore needs two updates: `latest` takes it to the v0.2.0
bridge, and `latest-v2` takes it to the current release. An explicit update
from v0.1.0 straight to v0.3.0 or later is rejected by design. The retired
key's public half and one manifest it signed are kept under
`tests/fixtures/retired-key/` so the CLI and both installers keep proving they
reject it.

Delete the retired private key from every secret store only after a disposable
v0.1.0 binary has updated through v0.2.0 to v0.3.0 and public acceptance passes
with both public keys.

A future rotation repeats this pattern, but it needs a **new pointer**, because
v0.3.0 and later clients read `latest-v2` and would otherwise skip the bridge
and reject it:

1. Ship a bridge release that trusts the current and next key and is signed by
   the current key, then freeze `latest-v2` on that bridge forever.
2. Ship the next release trusting only the new key, and advance a fresh
   `latest-v3` pointer that the new clients and installers read.

Freezing the old pointer is what lets a dormant client cross the rotation in
two updates. Never repoint a frozen pointer at a release its clients cannot
verify.

# mPaka CLI release operations

The CLI distribution is served at `https://cli.mpaka.dev`. Versioned
release objects are immutable; `latest` is the only mutable pointer.

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
  outside Git and never printed or committed.
- Secret `MPAKA_CLI_CLOUDFLARE_API_TOKEN`: scoped to deploy the distribution
  Worker and write the `mpaka-cli-releases` bucket.
- Variable `MPAKA_CLI_CLOUDFLARE_ACCOUNT_ID`: the Cloudflare account ID.

Pull requests generate a temporary test key and never publish. A production
publication occurs only for an annotated `vMAJOR.MINOR.PATCH` tag matching the
workspace version.

## Release and rollback

The tag workflow builds and smoke-tests all six targets, signs the exact
`manifest.json` bytes, publishes `releases/<version>/`, verifies every public
object, and only then writes `latest`.

To roll back, choose an already published version and update only the R2
`latest` object to that exact `vX.Y.Z` line. Do not overwrite or delete the
version prefix. Then run:

```bash
MPAKA_CLI_PUBLIC_KEY_FILE=/secure/path/mpaka-release-public.pem \
MPAKA_CLI_PUBLIC_BASE_URL=https://cli.mpaka.dev \
  scripts/cli-public-acceptance.sh
```

If verification fails, restore the previous `latest` value and investigate the
Worker/R2 or Access change before attempting another rollback.

## Key rotation

Generate a new RSA-3072 key outside Git, ship its public key in a reviewed CLI
and installer release, configure the new protected signing secret, and publish
only after the new verifier has passed PR and staging checks. Do not replace the
public key in place without a corresponding installer/CLI release.

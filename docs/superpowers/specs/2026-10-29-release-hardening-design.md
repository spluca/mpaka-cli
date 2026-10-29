# Release Hardening Design

## Intent

Make the standalone `spluca/mpaka-cli` repository the authoritative and safe
source for future CLI releases while matching the current public mPaka
topology.

## Goals

- Treat `https://mpaka.dev` as a public landing page.
- Keep `https://app.mpaka.dev` protected by Cloudflare Access.
- Keep `https://cli.mpaka.dev` public and verify signed release artifacts.
- Verify `https://api.mpaka.dev/health` remains publicly healthy.
- Exercise the PowerShell installer with real PowerShell in CI.
- Harden GitHub branch, dependency, secret, and Actions settings.
- Prepare future releases without republishing or mutating `v0.1.0`.

## Non-goals and anti-goals

- Do not rotate the RSA signing key or generate a replacement implicitly.
- Do not recreate the existing `v0.1.0` release prefix or move `latest`.
- Do not publish a tag until both existing production secrets are configured.
- Do not expose secret values in logs, files, commits, or messages.

## Constraints

- Versioned R2 release objects are immutable.
- Production publication remains restricted to annotated `v*` tags through the
  protected `release` environment.
- Untrusted fork code must never execute on the self-hosted runner.
- GitHub-hosted Windows may execute the fixture-only PowerShell installer test.

## Approaches

Two public-topology approaches were considered: restore Access on the landing,
or make the acceptance contract reflect the already-public landing while
checking Access on the console. The second approach was selected because the
owner confirmed `mpaka.dev` is intentionally public.

For Windows coverage, installing PowerShell on the persistent self-hosted
runner was rejected in favor of a separate GitHub-hosted Windows job. This
avoids mutating runner state and keeps the existing untrusted-fork boundary
around all self-hosted jobs.

For release provenance, recreating `v0.1.0` was rejected because the public
objects are immutable and a rebuild need not be byte-identical. The next
release will be the first tag published by this standalone repository after
its protected secrets are restored.

## Result

The repository will carry executable topology checks, real cross-platform
installer coverage, pinned third-party Actions, and documentation that makes
the migration boundary explicit. GitHub configuration will enforce reviewed
changes and enable the security controls available to this public repository.

# Release Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the standalone mPaka CLI repository ready to publish its next signed release with verified public topology and hardened CI/GitHub controls.

**Architecture:** Keep the live acceptance script as the authoritative end-to-end topology probe, and add a GitHub-hosted Windows fixture job alongside the guarded self-hosted build pipeline. Apply repository security settings through GitHub APIs, while treating signing and Cloudflare credentials as opaque external inputs that may only be reused, never regenerated implicitly.

**Tech Stack:** Bash, PowerShell, Rust, GitHub Actions, GitHub environments/API, Cloudflare Workers/R2.

**Spec:** `docs/superpowers/specs/2026-10-29-release-hardening-design.md`

## Global Constraints

- Do not rotate the RSA release signing key.
- Do not recreate, overwrite, retag, or republish `v0.1.0`.
- Keep all self-hosted jobs unavailable to fork pull requests.
- Never print or commit production secret values.
- `mpaka.dev` is public, `app.mpaka.dev` is Access-protected, `cli.mpaka.dev` is public, and `api.mpaka.dev/health` is public.

## Review Focus

- A public landing must return usable HTML rather than an Access redirect or error.
- The console check must prove the redirect is specifically from Cloudflare Access.
- Release verification must reject a changed signature or artifact hash before topology checks.
- Fork pull requests must not acquire a path to the self-hosted runner through job dependencies.
- Missing production secrets must block publication without blocking ordinary verification builds.

---

### Task 1: Public topology contract

**Files:**
- Modify: `scripts/cli-public-acceptance.sh`
- Modify: `docs/release.md`

**Interfaces:**
- Consumes: public HTTP endpoints and the configured release public key.
- Produces: a single acceptance command that verifies signed artifacts and all four public host boundaries.

- [ ] **Step 1: Change the landing assertion to require HTTP 200 and HTML.**
- [ ] **Step 2: Add an anonymous `app.mpaka.dev` assertion requiring a Cloudflare Access redirect.**
- [ ] **Step 3: Add an `api.mpaka.dev/health` assertion requiring HTTP 200.**
- [ ] **Step 4: Update the release runbook with the same topology.**
- [ ] **Step 5: Run acceptance with the embedded production public key and confirm it passes.**
- [ ] **Step 6: Commit the independently verified topology change.**

### Task 2: Real Windows installer coverage and pinned Actions

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `scripts/cli-release-workflow-test.sh`
- Modify: `docs/release.md`

**Interfaces:**
- Consumes: `scripts/installer-test.ps1` and immutable Action commit SHAs.
- Produces: a GitHub-hosted Windows fixture job required before packaging and a static workflow contract that preserves fork isolation.

- [ ] **Step 1: Extend the workflow contract test to require the Windows fixture job and exact package dependency.**
- [ ] **Step 2: Run the workflow test and confirm it fails against the current workflow.**
- [ ] **Step 3: Add the Windows job and make package depend on it without using the self-hosted runner.**
- [ ] **Step 4: Resolve every third-party Action tag to an immutable commit SHA and retain a version comment.**
- [ ] **Step 5: Update the runbook to distinguish fixture execution from cross-compilation.**
- [ ] **Step 6: Run workflow, PowerShell-static, and full local checks.**
- [ ] **Step 7: Commit the CI hardening change.**

### Task 3: GitHub release and repository controls

**Files:**
- No repository file changes expected.

**Interfaces:**
- Consumes: GitHub repository/environment administration APIs and existing opaque credentials if available.
- Produces: protected `main`, enabled security features, pinned-Actions policy where compatible, and a release-readiness audit.

- [ ] **Step 1: Confirm the `release` environment remains restricted to `v*` and `verify` has no secrets.**
- [ ] **Step 2: Locate existing secret sources by name only; configure the release environment only if values are securely available.**
- [ ] **Step 3: Enable secret scanning, push protection, Dependabot security updates, and SHA pinning where supported.**
- [ ] **Step 4: Protect `main` with pull-request review and required current CI checks without granting self-hosted execution to forks.**
- [ ] **Step 5: Re-query every setting and record any platform or credential blocker.**

### Task 4: Verification and delivery

**Files:**
- Verify all files changed by Tasks 1 and 2.

**Interfaces:**
- Consumes: the completed branch and external settings audit.
- Produces: a reviewed branch ready for pull request; no release tag.

- [ ] **Step 1: Run `make check`, Worker tests/dry-run, public acceptance, and `git diff --check`.**
- [ ] **Step 2: Review the complete diff for secrets, fork-runner regressions, and scope drift.**
- [ ] **Step 3: Push the branch and open a pull request with verification evidence.**
- [ ] **Step 4: Wait for CI and resolve failures within the approved scope.**
- [ ] **Step 5: Merge only after required checks pass; do not create a release tag.**

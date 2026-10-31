# mPaka CLI Release Signing Key Rotation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rotate the mPaka CLI release-signing key through a v0.2.0 compatibility bridge and a v0.3.0 new-key-only release without stranding v0.1.0 clients.

**Architecture:** v0.2.0 is prepared once in `spluca/mpaka-cli`, its exact canonical manifest is signed by the non-exportable old secret through a temporary workflow in `antpard/rustbox`, and the exact prepared bundle is published after verification. The legacy `latest` pointer remains fixed at v0.2.0, while v0.2.0 and all later clients/installers use `latest-v2`; v0.3.0 then removes the old trust root and resumes the normal tagged release pipeline with the new key.

**Tech Stack:** Rust 2024, RSA PKCS#1 v1.5 with SHA-256, Bash, PowerShell, GitHub Actions, Cloudflare Workers/R2, `gh`, OpenSSL, pnpm/Node.js.

**Spec:** `docs/superpowers/specs/2026-10-31-release-signing-key-rotation-design.md`

## Global Constraints

- Never print, commit, upload as an artifact, or place in shell history either private key.
- Generate a new RSA-3072 private key only at `/home/apardo/rustbox/rustbox/.secrets/mpaka-cli-release-signing-key.pem`; directory mode is `0700`, file mode is `0600`.
- Keep RSA PKCS#1 v1.5 with SHA-256 and the single detached `manifest.json.sig` format.
- v0.2.0 must accept both the old and new public keys; v0.3.0 must accept only the new public key.
- `latest` becomes an immutable-in-practice compatibility pointer to v0.2.0; `latest-v2` is the only pointer advanced by v0.3.0 and later releases.
- Release prefixes remain immutable and contain exactly the six existing target archives plus `manifest.json` and `manifest.json.sig`.
- Do not overwrite v0.1.0, v0.2.0, or any conflicting public release prefix.
- Use isolated worktrees in both repositories and preserve unrelated modified or untracked files.
- A code commit, push, PR, merge, workflow dispatch, secret mutation, tag, publication, or cleanup occurs only at its named gate and only after the preceding verification passes.

## Review Focus

- A v0.1.0 binary first run after v0.3.0 exists must still receive v0.2.0 from `latest`, then receive v0.3.0 from `latest-v2` on its next update; Task 9 performs this end-to-end test from a disposable copy.
- An explicit v0.1.0 jump to v0.3.0 must reject the new-key-only signature instead of bypassing the bridge; Task 9 tests this failure before the normal two-step update.
- The bytes signed by the old repository must be the same bytes published under v0.2.0; Tasks 4 and 6 pin the preparation run ID, source commit, manifest SHA-256, signature, and bundle contents.
- v0.3.0 must reject an otherwise valid manifest signed only by the old key on Rust, Bash, and PowerShell paths; Task 8 adds the rejection fixtures before removing dual trust.
- A failed upload or verification must not advance either pointer; Tasks 4 and 6 test ordering and verify public objects before pointer writes.

---

### Task 1: Add guarded old-key signing and local key custody to RustBox

**Repository:** `/home/apardo/rustbox/rustbox` in a new `.worktrees/mpaka-signing-bridge` worktree.

**Files:**
- Modify: `.gitignore`
- Create: `scripts/mpaka-cli-bridge-manifest.sh`
- Create: `scripts/mpaka-cli-bridge-manifest-test.sh`
- Create: `.github/workflows/mpaka-cli-bridge-sign.yml`

**Interfaces:**
- Consumes: base64-encoded canonical `manifest.json`, its lowercase SHA-256, and the existing repository secret `MPAKA_CLI_RELEASE_SIGNING_KEY`.
- Produces: an Actions artifact named `mpaka-cli-v0.2.0-signature` containing only `manifest.json.sig` and `manifest.sha256`.

- [ ] **Step 1: Create the RustBox worktree and verify its clean baseline**

Run from `/home/apardo/rustbox/rustbox`:

```bash
git status --short --branch
git check-ignore -q .worktrees
git worktree add .worktrees/mpaka-signing-bridge -b security/mpaka-signing-bridge
```

Expected: the shared checkout is unchanged and the new branch starts from current `main`.

- [ ] **Step 2: Write failing bridge-manifest contract tests**

In `scripts/mpaka-cli-bridge-manifest-test.sh`, assert that validation:

- accepts only canonical `jq -c -S` JSON for version `v0.2.0`;
- requires the six exact target/filename pairs with no duplicates;
- rejects a wrong version, malformed SHA-256, a hash mismatch, non-canonical bytes, missing/extra targets, and unsafe filenames;
- signs with a generated fixture key and verifies the detached signature against its derived public key;
- rejects a signing key whose public-key fingerprint does not equal the supplied expected fingerprint;
- leaves no private key in the output directory.

- [ ] **Step 3: Run the tests and verify they fail**

Run:

```bash
bash scripts/mpaka-cli-bridge-manifest-test.sh
```

Expected: FAIL because `scripts/mpaka-cli-bridge-manifest.sh` does not exist.

- [ ] **Step 4: Implement the manifest validator/signer**

Create `scripts/mpaka-cli-bridge-manifest.sh` with these commands:

```text
validate --manifest FILE --sha256 HEX
sign --manifest FILE --sha256 HEX --signing-key FILE --expected-key-fingerprint HEX --output-dir DIR
```

`sign` must call the same validation path, use `openssl dgst -sha256 -sign`, emit only the signature and manifest hash, and never copy the private key to the output directory.

- [ ] **Step 5: Add the temporary GitHub signing workflow**

Create `.github/workflows/mpaka-cli-bridge-sign.yml` with `pull_request` contract checks and a guarded `workflow_dispatch`. The dispatch inputs are `manifest_base64` and `manifest_sha256`; the signing job must require `github.actor == github.repository_owner`, use only SHA-pinned third-party Actions, restrict permissions to `contents: read`, validate before injecting the secret, write the secret under `umask 077`, compare the derived public-key fingerprint to `fec4af38209410541558e098f79a11eb3a405512ea30a0f8b7106735623296e0`, remove the key before upload, and retain the signature artifact for one day.

- [ ] **Step 6: Add and test the durable ignored key directory**

Add `/.secrets/` to `.gitignore`, then run:

```bash
git check-ignore -v .secrets/mpaka-cli-release-signing-key.pem
bash scripts/mpaka-cli-bridge-manifest-test.sh
git diff --check
```

Expected: `.secrets/...pem` is ignored, all fixture tests pass, and the diff is clean.

- [ ] **Step 7: Run proportionate RustBox verification**

Run the new fixture test and the repository's existing workflow/static checks that cover `.github/workflows`; if no focused target exists, run the documented CI command relevant to shell/workflow changes. Record exact commands and results in the PR body.

- [ ] **Step 8: Commit the reviewed RustBox change**

```bash
git add .gitignore scripts/mpaka-cli-bridge-manifest.sh scripts/mpaka-cli-bridge-manifest-test.sh .github/workflows/mpaka-cli-bridge-sign.yml
git commit -m "build: add guarded mPaka bridge signer"
```

Do not push or open/merge a PR until explicitly authorized.

### Task 2: Generate and verify the new durable RSA key

**Repository:** `/home/apardo/rustbox/rustbox` shared checkout, only after Task 1's `.gitignore` change is merged into `main`.

**Files:**
- Create, ignored: `.secrets/mpaka-cli-release-signing-key.pem`
- Create, ignored: `.secrets/mpaka-cli-release-signing-key.pub.pem`
- Create, ignored: `.secrets/mpaka-cli-release-signing-key.fingerprint`

**Interfaces:**
- Produces: the private key used by GitHub after the bridge, its public PEM embedded in v0.2.0/v0.3.0, and its lowercase SPKI SHA-256 fingerprint.

- [ ] **Step 1: Prove the destination is ignored before creating secret material**

Run:

```bash
git check-ignore -v .secrets/mpaka-cli-release-signing-key.pem
```

Expected: the tracked root `.gitignore` rule `/.secrets/` matches.

- [ ] **Step 2: Generate the key without exposing it**

Use `umask 077`, create `.secrets` with mode `0700`, and run OpenSSL `genpkey` with RSA and `rsa_keygen_bits:3072` directly to the agreed PEM path. Derive the public PEM with `openssl pkey -pubout`; do not route the private PEM through stdout, command substitution, or a tool response.

- [ ] **Step 3: Validate custody and cryptographic identity**

Run checks that assert directory mode `700`, private-file mode `600`, `openssl pkey -check -noout` succeeds, the public key reports 3072 bits, and the SHA-256 of the DER SubjectPublicKeyInfo is written to the ignored `.fingerprint` file. Print only the fingerprint.

- [ ] **Step 4: Verify Git cannot see the key files**

Run:

```bash
git status --short --ignored .secrets
git check-ignore -v .secrets/mpaka-cli-release-signing-key.pem
```

Expected: `.secrets/` is ignored and no secret appears as an untracked or tracked file. Do not commit anything in this task.

### Task 3: Implement v0.2.0 dual trust and the `latest-v2` channel

**Repository:** `/home/apardo/rustbox/mpaka-cli/.worktrees/signing-key-rotation`.

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/distribution.rs`
- Modify: `src/update.rs`
- Modify: `install`
- Modify: `install.ps1`
- Modify: `scripts/installer-test.sh`
- Modify: `scripts/installer-test.ps1`
- Modify: `scripts/installer-test-static.sh`
- Modify: `cli-distribution/src/index.js`
- Modify: `cli-distribution/test/worker.test.mjs`
- Modify: `scripts/cli-public-acceptance.sh`
- Modify: `scripts/cli-public-acceptance-test.sh`
- Modify: `scripts/cli-docs-test.sh`
- Modify: `docs/cli.md`
- Modify: `docs/release.md`
- Modify: `README.md` only if its user-facing update description needs the two-step v0.1.0 note.

**Interfaces:**
- Produces: `verify_manifest_signature` accepting old or new key for v0.2.0; implicit CLI/installer lookup at `latest-v2`; Worker support for both pointer objects; public acceptance that verifies compatibility and active channels independently.
- Consumes: the new public PEM and fingerprint from Task 2; the old embedded public key already in the repository.

- [ ] **Step 1: Write failing Rust dual-trust and channel tests**

Add tests named:

```text
accepts_manifests_signed_by_either_bridge_key
rejects_manifests_signed_by_an_untrusted_key
bridge_clients_use_latest_v2_for_implicit_updates
```

Use the existing fixed old-key signature and a signature over the same fixed manifest produced once with the new private key; commit only the public PEM and detached signature fixture. Assert changed manifest bytes fail for both signatures.

- [ ] **Step 2: Run the focused Rust tests and verify they fail**

Run:

```bash
cargo nextest run -E 'test(accepts_manifests_signed_by_either_bridge_key) | test(rejects_manifests_signed_by_an_untrusted_key) | test(bridge_clients_use_latest_v2_for_implicit_updates)'
```

Expected: FAIL because only the old key and `latest` are implemented.

- [ ] **Step 3: Implement Rust bridge verification and pointer selection**

In `src/distribution.rs`, name the trust roots explicitly as old and new bridge keys and have `verify_manifest_signature(manifest_bytes, signature_bytes)` succeed if either trusted key verifies the same detached signature. Keep malformed-key/signature errors fail-closed. In `src/update.rs`, add a single `LATEST_POINTER: &str = "latest-v2"` constant and use it only when no explicit version is requested.

- [ ] **Step 4: Write failing Bash installer tests**

Extend `scripts/installer-test.sh` to serve `latest-v2`, verify that implicit installation requests it, accept manifests signed by either of two fixture keys through the test-only key-list hook, and reject an unrelated signature. Extend the static test to assert both production public keys and the `latest-v2` literal are present.

- [ ] **Step 5: Run Bash installer tests and verify they fail**

Run:

```bash
bash scripts/installer-test.sh
bash scripts/installer-test-static.sh
```

Expected: FAIL until the installer supports a key list and `latest-v2`.

- [ ] **Step 6: Implement Bash dual trust and `latest-v2`**

Keep the production old and new PEMs as separate readonly values. Add a verification function that writes each candidate public key to a mode-`0600` temporary file and succeeds only when one key verifies; retain a test-only override that cannot affect production unless explicitly set. Change only implicit version selection to `latest-v2`; `--version` continues to address immutable release prefixes directly.

- [ ] **Step 7: Write failing PowerShell dual-trust tests**

In `scripts/installer-test.ps1`, generate two fixture RSA keys in memory, sign the same manifest with each, and assert `Test-ManifestSignature` accepts either trusted public key and rejects a third. Add an assertion that the implicit pointer constant is `latest-v2`.

- [ ] **Step 8: Run PowerShell tests and verify they fail**

Run on Windows CI, and locally if `pwsh` is available:

```powershell
pwsh -NoProfile -File scripts/installer-test.ps1
```

Expected: FAIL because `Test-ManifestSignature` accepts one PEM and the installer uses `latest`.

- [ ] **Step 9: Implement PowerShell dual trust and `latest-v2`**

Change `Test-ManifestSignature` to consume an array of public PEMs and return true on the first valid signature, false otherwise; dispose every RSA instance. Keep the test-only override explicit. Use `latest-v2` only for implicit version selection.

- [ ] **Step 10: Add Worker and public-acceptance channel tests**

In `cli-distribution/test/worker.test.mjs`, assert `/latest-v2` maps to R2, is served with the short mutable cache policy, and traversal/unknown variants remain rejected. In `scripts/cli-public-acceptance-test.sh`, fixture both pointers and assert the compatibility channel must equal v0.2.0 while the active channel may advance.

- [ ] **Step 11: Implement Worker and acceptance support**

Allow the exact `latest-v2` object in `cli-distribution/src/index.js`. Update `scripts/cli-public-acceptance.sh` to verify:

- `latest` equals an explicit compatibility version (v0.2.0 after bridge publication) and its manifest verifies with the legacy public key;
- `latest-v2` selects the active version and its manifest verifies with the active public key;
- all six artifacts for both selected versions match their signed manifests, deduplicating downloads when both pointers equal v0.2.0.

- [ ] **Step 12: Update user and operator documentation tests first, then docs**

Make `scripts/cli-docs-test.sh` require the two-step v0.1.0 update behavior and the permanent `latest`/active `latest-v2` operator contract. Update `docs/cli.md` in task-first language and `docs/release.md` with pointer invariants and rollback rules. Do not expose infrastructure details beyond what an operator needs.

- [ ] **Step 13: Bump the bridge version**

Set `Cargo.toml` to `0.2.0`, update `Cargo.lock` through Cargo, and verify `mpaka --version` reports `0.2.0`.

- [ ] **Step 14: Run focused verification**

Run:

```bash
cargo fmt --all -- --check
cargo nextest run --locked
bash scripts/installer-test.sh
bash scripts/installer-test-static.sh
pnpm --dir cli-distribution test
bash scripts/cli-docs-test.sh
bash scripts/cli-public-acceptance-test.sh
git diff --check
```

Expected: all pass; PowerShell behavior remains pending the required Windows CI job if local `pwsh` is unavailable.

### Task 4: Add exact-bundle bridge preparation and protected publication

**Files:**
- Modify: `.github/workflows/release.yml`
- Create: `.github/workflows/bridge-publish.yml`
- Modify: `scripts/cli-release-workflow-test.sh`
- Modify: `docs/release.md`

**Interfaces:**
- Produces: a retained `mpaka-cli-v0.2.0-prepared` artifact from an exact source ref; a protected publisher consuming `prepare_run_id`, `expected_commit`, `manifest_sha256`, and `manifest_signature_base64`.
- Consumes: the six existing cross-target build artifacts, the old-key signature artifact from Task 1, and the release environment Cloudflare secret.

- [ ] **Step 1: Write failing workflow contract assertions**

Extend `scripts/cli-release-workflow-test.sh` to require:

- `workflow_dispatch` preparation input fixed to `v0.2.0`;
- an uploaded bundle containing the six archives, canonical manifest, both installers, source commit, and manifest hash, but no private key and no test signature;
- normal tag publication explicitly skipping v0.2.0;
- a separate `bridge-publish.yml` with `actions: read`, `contents: read`, `environment: release`, owner-only dispatch, exact run/commit/hash/signature inputs, old-key verification, immutable-prefix refusal, byte-for-byte public verification, and pointer writes last;
- all third-party Actions pinned to full commit SHAs and all self-hosted jobs guarded from fork PRs.

- [ ] **Step 2: Run the workflow contract and verify it fails**

Run:

```bash
bash scripts/cli-release-workflow-test.sh
```

Expected: FAIL because bridge preparation/publication is absent.

- [ ] **Step 3: Add v0.2.0 preparation to the existing release workflow**

Add a manual preparation mode to `.github/workflows/release.yml` that reuses the existing contract, PowerShell, six-target build, and packaging jobs. For workflow dispatch it must package version v0.2.0 with an ephemeral test key only to obtain canonical manifest bytes, delete the ephemeral key and signature, record `GITHUB_SHA`, and upload the exact bundle for seven days without deploying, publishing, tagging, or changing pointers. Normal tag jobs must refuse/skip v0.2.0 so the bridge cannot accidentally enter the new-key pipeline.

- [ ] **Step 4: Implement protected bridge publication**

Create `.github/workflows/bridge-publish.yml` that downloads the named artifact from the exact preparation run, requires its recorded commit to equal `expected_commit`, recomputes the manifest SHA-256, decodes the public signature input, verifies it with the old embedded public key, validates all archive hashes/sizes, deploys the Worker from `expected_commit`, publishes the immutable v0.2.0 prefix and prepared installers, compares every public object byte-for-byte, then writes and verifies both `latest` and `latest-v2` as v0.2.0. It must not contain or reference either private-key secret.

- [ ] **Step 5: Add rollback/order assertions**

The workflow contract must fail if pointer writes appear before public object comparison, if v0.2.0 is not hard-coded at all bridge boundaries, or if a conflicting existing manifest can be overwritten.

- [ ] **Step 6: Run workflow and full local verification**

Run:

```bash
bash scripts/cli-release-workflow-test.sh
make check
pnpm --dir cli-distribution install --frozen-lockfile
pnpm --dir cli-distribution exec wrangler deploy --dry-run
git diff --check
```

Expected: every local gate passes.

### Task 5: Review and deliver the bridge code changes

**Files:** all mPaka CLI files changed by Tasks 3 and 4 plus the approved design and this plan.

**Interfaces:**
- Produces: a reviewable v0.2.0 bridge commit/PR in `spluca/mpaka-cli`.

- [ ] **Step 1: Run the complete pre-commit gate**

Run `make check`, the Worker install/test/dry-run commands, `git diff --check`, and inspect `git diff --stat` plus `git status --short`. Confirm no PEM private key, `.secrets` content, generated release bundle, or unrelated file is present.

- [ ] **Step 2: Commit the bridge implementation**

```bash
git add Cargo.toml Cargo.lock src install install.ps1 scripts cli-distribution .github docs README.md
git commit -m "build: add signing key rotation bridge"
```

Include `README.md` only if Task 3 changed it. Do not stage unrelated files.

- [ ] **Step 3: Obtain an independent security/release review**

Review focus: trust-root ordering, exact artifact identity, secret exposure, fork/self-hosted guards, immutable-prefix behavior, pointer ordering, Windows parity, and the dormant v0.1.0 path. Address findings with focused tests and rerun the full gate.

- [ ] **Step 4: Stop at the delivery gate**

Report both local commits and verification evidence. Push branches, open PRs, wait for CI, merge, and clean worktrees only after explicit authorization for those lifecycle actions.

### Task 6: Publish the v0.2.0 compatibility bridge

**Prerequisites:** both Task 1 and Task 5 PRs merged; new key from Task 2 verified; `main` clean and synchronized in both repositories; publication explicitly authorized.

**External state:** GitHub tags/workflows/artifacts, Cloudflare Worker/R2, public `cli.mpaka.dev` pointers.

- [ ] **Step 1: Verify pre-publication state**

Confirm `cli.mpaka.dev/latest` is v0.1.0, `releases/v0.2.0/manifest.json` is absent, the old secret metadata still exists only in `antpard/rustbox`, the new repository release environment still lacks the signing-key secret, and both merged commits are on their respective `main` branches.

- [ ] **Step 2: Create and push tag v0.2.0 at the reviewed mPaka commit**

Verify the tag target and push only the exact tag after authorization. Confirm the normal tag workflow runs contract/build checks but does not sign or publish v0.2.0.

- [ ] **Step 3: Dispatch bridge preparation at tag v0.2.0**

Run the manual preparation workflow with ref `v0.2.0`. Wait for every contract, Windows, and six-target build job to pass. Record the preparation run ID, source commit, artifact ID, and manifest SHA-256.

- [ ] **Step 4: Independently inspect the prepared bundle**

Download the artifact to a fresh temporary directory. Verify it contains exactly six archives, `manifest.json`, both installers, source commit, and manifest hash; verify every archive hash/size and confirm there is no `.pem`, private key, or signature.

- [ ] **Step 5: Dispatch old-key signing without exposing the manifest or key in logs**

Base64-encode the canonical manifest into a temporary input file, dispatch `.github/workflows/mpaka-cli-bridge-sign.yml` in `antpard/rustbox`, and wait for success. Download the signature artifact, assert it contains only `manifest.json.sig` and `manifest.sha256`, match the hash to the preparation run, and verify the signature locally against the old public key.

- [ ] **Step 6: Dispatch protected bridge publication**

Pass the recorded preparation run ID, exact commit, manifest hash, and public signature to `bridge-publish.yml`. Approve the `release` environment only after its preflight summary matches the recorded evidence. Wait for publication and pointer verification to finish.

- [ ] **Step 7: Run public bridge acceptance**

Verify:

```text
/latest     = v0.2.0
/latest-v2  = v0.2.0
```

Run `scripts/cli-public-acceptance.sh` with the old key as both legacy and active verifier, verify all six artifacts, install through the public Bash path, rely on the green Windows installer CI fixture for PowerShell plus inspect the public script bytes, and run a disposable public v0.1.0 binary through `mpaka self-update` to confirm it becomes v0.2.0.

- [ ] **Step 8: Preserve evidence and stop before cutover**

Record workflow run URLs/IDs, commit/tag SHA, manifest SHA-256, old/new public fingerprints, public acceptance output, and pointer values. Do not set the new secret or delete the old secret yet.

### Task 7: Remove the one-use signer from RustBox

**Repository:** `/home/apardo/rustbox/rustbox` in a fresh worktree after Task 6 succeeds.

**Files:**
- Delete: `.github/workflows/mpaka-cli-bridge-sign.yml`
- Delete: `scripts/mpaka-cli-bridge-manifest.sh`
- Delete: `scripts/mpaka-cli-bridge-manifest-test.sh`
- Preserve: `.gitignore` rule `/.secrets/`

**Interfaces:**
- Produces: RustBox `main` with no old-key signing oracle; the old secret remains inert until final acceptance.

- [ ] **Step 1: Write/perform the cleanup assertions**

Assert the three temporary files are absent, `.secrets` remains ignored, the local new key still has modes `0700/0600`, and no unrelated RustBox file changes.

- [ ] **Step 2: Commit the cleanup**

```bash
git add -u .github/workflows/mpaka-cli-bridge-sign.yml scripts/mpaka-cli-bridge-manifest.sh scripts/mpaka-cli-bridge-manifest-test.sh
git commit -m "build: remove one-use mPaka bridge signer"
```

- [ ] **Step 3: Stop at the RustBox delivery gate**

Push/PR/merge only with explicit authorization. Do not delete `MPAKA_CLI_RELEASE_SIGNING_KEY` yet.

### Task 8: Implement v0.3.0 new-key-only trust and remove bridge machinery

**Repository:** `/home/apardo/rustbox/mpaka-cli` in a fresh worktree based on merged v0.2.0 `main`.

**Files:**
- Modify: `Cargo.toml`
- Modify: `Cargo.lock`
- Modify: `src/distribution.rs`
- Modify: `install`
- Modify: `install.ps1`
- Modify: signature tests under `src/distribution.rs`, `scripts/installer-test.sh`, and `scripts/installer-test.ps1`
- Modify: `.github/workflows/release.yml`
- Delete: `.github/workflows/bridge-publish.yml`
- Modify: `scripts/cli-release-workflow-test.sh`
- Modify: `docs/release.md`
- Modify: `docs/cli.md`

**Interfaces:**
- Produces: v0.3.0 clients/installers trusting only the new public key; the normal release pipeline signing with the new environment secret and advancing only `latest-v2`.
- Consumes: v0.2.0 public acceptance and the new public key from Task 2.

- [ ] **Step 1: Write failing old-key rejection tests**

Change Rust, Bash, and PowerShell fixtures to assert the new signature succeeds and the fixed valid old-key signature fails. Add workflow assertions that normal tags write only `latest-v2`, never `latest`, and that no bridge dispatch/publisher remains.

- [ ] **Step 2: Run focused tests and verify they fail**

Run the Rust signature test, Bash installer test, PowerShell test on Windows/local `pwsh`, and workflow contract. Expected: FAIL while dual trust and bridge machinery remain.

- [ ] **Step 3: Remove the old trust root everywhere**

Keep only the new public PEM in `src/distribution.rs`, `install`, and `install.ps1`. Simplify production verification to the single new key without weakening test-only injection boundaries. Preserve `latest-v2` as the implicit channel.

- [ ] **Step 4: Make the normal release pipeline authoritative**

Ensure `.github/workflows/release.yml` requires `MPAKA_CLI_RELEASE_SIGNING_KEY` from the protected `release` environment for every production tag at v0.3.0 and later, publishes immutable objects, verifies them, and updates only `mpaka-cli-releases/latest-v2`. Delete `bridge-publish.yml` and remove all preparation-only paths/inputs from `release.yml` after their v0.2.0 evidence is recorded.

- [ ] **Step 5: Bump to v0.3.0 and update docs**

Set Cargo version/lockfile to 0.3.0. Document that v0.1.0 users run the normal update twice when arriving after cutover; do not instruct them to bypass signature verification or jump directly to v0.3.0.

- [ ] **Step 6: Run the full v0.3.0 gate**

Run:

```bash
make check
pnpm --dir cli-distribution install --frozen-lockfile
pnpm --dir cli-distribution test
pnpm --dir cli-distribution exec wrangler deploy --dry-run
git diff --check
```

Expected: all local checks pass; Windows CI must pass before merge.

- [ ] **Step 7: Commit, review, and stop at delivery**

```bash
git add Cargo.toml Cargo.lock src install install.ps1 scripts cli-distribution .github docs README.md
git commit -m "build: complete release signing key rotation"
```

Obtain an independent security/release review, rerun the full gate after fixes, and wait for explicit push/PR/merge authorization.

### Task 9: Publish v0.3.0 and retire the old key

**Prerequisites:** Tasks 7 and 8 merged; v0.2.0 remains publicly accepted; final publication and destructive secret deletion explicitly authorized.

**External state:** `spluca/mpaka-cli` release environment secret, v0.3.0 tag/workflow, Cloudflare R2 pointers, old RustBox secret.

- [ ] **Step 1: Verify the local new key one final time**

Assert ignored path, modes, RSA-3072 integrity, and fingerprint equality with the sole public key embedded in merged v0.3.0 code and both installers. Print only the fingerprint.

- [ ] **Step 2: Set the new protected release secret securely**

Feed the PEM through stdin from the ignored file:

```bash
gh secret set MPAKA_CLI_RELEASE_SIGNING_KEY --repo spluca/mpaka-cli --env release < /home/apardo/rustbox/rustbox/.secrets/mpaka-cli-release-signing-key.pem
```

Verify only secret metadata with `gh secret list`; never echo or read the secret back.

- [ ] **Step 3: Tag and publish v0.3.0**

Create v0.3.0 at the exact reviewed merge commit, verify the tag target, push only that tag, and monitor every GitHub Actions job. The release job must sign with the new key, publish and verify all immutable objects, leave `latest` untouched, and advance `latest-v2` only after verification.

- [ ] **Step 4: Run new-key public acceptance**

Run the public acceptance script with the old public key for `latest`/v0.2.0 and the new public key for `latest-v2`/v0.3.0. Verify public Bash installation and the green Windows installer job. Confirm all six v0.3.0 archive hashes and sizes.

- [ ] **Step 5: Prove the dormant-client chain from a disposable v0.1.0 binary**

Download the immutable public v0.1.0 archive to a fresh temporary directory and verify it against the v0.1.0 old-key manifest. First assert an explicit update to v0.3.0 fails signature verification. Then run normal self-update once and assert the copied binary becomes v0.2.0; run it again and assert it becomes v0.3.0. Do not exercise the user's installed executable.

- [ ] **Step 6: Prove old-key retirement**

Against a local fixture server, serve a valid v0.3.0 manifest signed only by the old fixture key and assert the public v0.3.0 binary, Bash installer, and PowerShell test path reject it.

- [ ] **Step 7: Delete the old repository secret only after every acceptance gate passes**

Delete `MPAKA_CLI_RELEASE_SIGNING_KEY` from `antpard/rustbox`, then verify its metadata is absent while the new `spluca/mpaka-cli` release-environment secret remains present. This deletion is irreversible from GitHub and requires explicit authorization at execution time.

- [ ] **Step 8: Final audit and handoff**

Confirm:

- `latest` is v0.2.0 and `latest-v2` is v0.3.0;
- v0.1.0 and v0.2.0 prefixes remain byte-for-byte immutable;
- old signer workflows/scripts are absent from both repository heads;
- no private PEM is tracked or present in workflow artifacts/logs;
- both repositories are clean and synchronized after any authorized cleanup;
- the local new private key remains mode `0600` at the agreed ignored path.

Report exact commits, tags, workflow runs, manifest hashes, public-key fingerprints, tests, pointer values, and secret metadata without exposing secret content.

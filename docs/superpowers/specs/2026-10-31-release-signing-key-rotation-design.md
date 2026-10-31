# mPaka CLI Release Signing Key Rotation Design

## Intent

Rotate the mPaka CLI release-signing key without breaking `mpaka self-update`
for the published v0.1.0 clients. The original private key is unavailable to
the operator but remains usable as the GitHub Actions secret
`MPAKA_CLI_RELEASE_SIGNING_KEY` in `antpard/rustbox`.

The rotation must finish with releases, installers, and the release workflow
trusting only a new RSA-3072 key. The new private key must have a durable local
operator copy at
`/home/apardo/rustbox/rustbox/.secrets/mpaka-cli-release-signing-key.pem`, mode
`0600`, and an operational copy in the protected `release` environment of
`spluca/mpaka-cli`.

## Goals

- Keep v0.1.0 self-update working through the rotation.
- Generate a fresh RSA-3072 key and verify its public-key fingerprint before use.
- Publish a bridge release, v0.2.0, whose manifest is signed by the old key and
  whose CLI and installers trust both the old and new public keys.
- Publish a final rotation release, v0.3.0, whose manifest is signed by the new
  key and whose CLI and installers trust only the new public key.
- Preserve immutable release prefixes, keep `latest` as the permanent v0.1.0
  compatibility bridge, and advance `latest-v2` only after public verification
  succeeds.
- Remove the old signing secret and all one-use signing machinery after public
  v0.3.0 acceptance succeeds.

## Non-goals

- Recovering, printing, downloading, or otherwise exporting the old private
  key from GitHub.
- Changing the RSA PKCS#1 v1.5 with SHA-256 manifest-signature format.
- Changing artifact targets, archive layout, installation locations, API
  behavior, or Cloudflare routing. One additional R2 pointer object,
  `latest-v2`, is part of this rotation.
- Re-signing or overwriting v0.1.0.

## Anti-goals

- A direct key replacement that makes existing clients reject the next release.
- Leaving both trust roots accepted indefinitely.
- Publishing a manifest signed for artifacts other than the exact artifacts
  that are subsequently uploaded.
- Writing either private key to logs, workflow artifacts, commits, shell
  history, or world-readable storage.

## Constraints

- v0.1.0 embeds exactly the old public key and fetches
  `releases/<version>/manifest.json.sig`; therefore the bridge manifest must be
  signed by the old key in the existing format.
- v0.1.0 also fetches `latest` directly. That pointer must remain v0.2.0 after
  cutover so a dormant v0.1.0 client never skips the bridge.
- The old key is a repository Actions secret in `antpard/rustbox`; GitHub does
  not allow reading it back.
- `spluca/mpaka-cli` has the Cloudflare publishing secret only in its protected
  `release` environment.
- `https://cli.mpaka.dev/latest` currently points to v0.1.0, whose public
  immutable release contains six artifacts.
- The exact prepared artifacts and manifest must cross the old-key signing
  step unchanged.
- All repository changes use isolated worktrees. Unrelated working-tree state
  is preserved.

## Approaches Considered

### 1. Two-release compatibility bridge — selected

Prepare v0.2.0 once, sign its exact manifest with the old GitHub secret, and
publish that exact prepared bundle. v0.2.0 trusts both keys. Then replace the
new repository's release secret with the new key and publish v0.3.0, which
trusts only the new key.

This preserves self-update for v0.1.0 and fully retires the old trust root. It
costs two releases, a permanent compatibility pointer, and temporary, narrowly
scoped workflow machinery.

### 2. Direct cutover

Publish a new-key-only release and require every existing user to reinstall.
This is operationally simpler but breaks the established self-update path and
was rejected.

### 3. Keep dual trust indefinitely

Publish a bridge and continue accepting the old key forever. This preserves
compatibility but does not complete a key rotation, so it was rejected.

## Design

### New key custody

Create `/home/apardo/rustbox/rustbox/.secrets/` with mode `0700`, add
`/.secrets/` to the root RustBox `.gitignore`, and generate the new private key
directly at the agreed path with mode `0600`. Derive the public key and its
SHA-256 SubjectPublicKeyInfo fingerprint without printing the private key.

Before any release, prove that the local private key derives the public key
embedded in the bridge code and installers. Set the same private PEM as the
`MPAKA_CLI_RELEASE_SIGNING_KEY` environment secret in `spluca/mpaka-cli` only
when the bridge is public and the final v0.3.0 release is ready to be signed.

### Bridge trust behavior

For v0.2.0, the Rust verifier, Bash installer, and PowerShell installer accept
a manifest when either the old or new embedded public key verifies the same
signature. Invalid signatures, malformed keys, and signatures made by an
unrelated key remain rejected. Tests pin all three surfaces to this behavior.

The v0.2.0 CLI and both v0.2.0 installers use `latest-v2` for implicit version
selection. During bridge publication, both `latest` and `latest-v2` are set to
v0.2.0, but they have different long-term roles:

- `latest` remains v0.2.0 permanently for v0.1.0 clients;
- `latest-v2` is the active stable channel for v0.2.0 and later clients and for
  all current installers.

A dormant v0.1.0 client can therefore run the normal self-update at any future
date and receive v0.2.0. A second normal self-update from v0.2.0 follows
`latest-v2` to the current release. An explicit attempt to jump directly from
v0.1.0 to a new-key-only version remains unsupported and fails closed.

The release manifest remains unchanged and carries one detached signature.
The bridge signature is produced with the old key, so v0.1.0 accepts v0.2.0.

### Exact-bundle bridge signing

The new repository prepares the v0.2.0 six-target bundle and canonical
manifest once and uploads them as a retained GitHub Actions artifact without
publishing them. The preparation run records the manifest SHA-256.

A temporary manual workflow in `antpard/rustbox` accepts the canonical
manifest bytes, permits only v0.2.0 with the six expected targets and archive
names, verifies the supplied SHA-256, signs those bytes with the old secret,
and returns only `manifest.json.sig`. The workflow never emits or transfers the
private key. It is restricted to the repository owner and removed immediately
after the signature is obtained.

A protected manual publisher in `spluca/mpaka-cli` downloads the exact bundle
from the recorded preparation run, verifies its manifest hash and old-key
signature, publishes the immutable v0.2.0 prefix and dual-trust installers,
verifies every public object byte-for-byte, and only then advances `latest`.
For the bridge only, it writes both `latest` and `latest-v2` as v0.2.0. It must
refuse an existing differing prefix.

### Final cutover

After v0.2.0 public acceptance, change the CLI and both installers to embed and
trust only the new public key, bump to v0.3.0, and set the new private key in
the `spluca/mpaka-cli` protected release environment. The normal tagged release
pipeline signs and publishes v0.3.0. From this release onward, the normal
pipeline updates only `latest-v2`; it must never advance `latest` beyond
v0.2.0.

Acceptance must prove all of the following:

- the original v0.1.0 binary accepts and applies v0.2.0;
- the v0.2.0 binary accepts and applies v0.3.0;
- the public Bash and PowerShell installers verify v0.3.0 with the new key;
- v0.3.0 rejects a manifest signed only by the old key;
- all six public v0.3.0 artifacts match the signed manifest;
- `latest` remains v0.2.0 and `latest-v2` equals v0.3.0 only after those checks
  pass;
- a fresh copy of the original v0.1.0 binary can still update to v0.2.0 after
  v0.3.0 is public, proving the dormant-client path does not depend on a soak
  period or adoption telemetry.

### Retirement and cleanup

Once v0.3.0 acceptance succeeds, delete
`MPAKA_CLI_RELEASE_SIGNING_KEY` from `antpard/rustbox`, remove the temporary
old-repository signing workflow, and remove bridge-only preparation/publishing
machinery that is no longer useful. Keep the immutable v0.1.0 and v0.2.0
release objects.

The local new private key and the new repository environment secret remain.
The public key fingerprint and rotation evidence may be documented; the
private PEM must never be included.

## Failure handling

- Before `latest` moves to v0.2.0, a failure leaves v0.1.0 authoritative and
  can be retried with the same prepared bundle.
- After v0.2.0 but before v0.3.0, v0.2.0 remains a valid stable bridge and both
  keys are usable; do not delete the old secret.
- If v0.3.0 publication or acceptance fails, keep both pointers at v0.2.0 and
  retain the old secret until the new-key release is verified.
- Never overwrite an immutable release prefix to recover from failure; create
  a new patch version if published content itself is defective.

## Delivery boundary

Implementation, commits, pushes, pull requests, merges, workflow dispatches,
secret changes, tags, release publication, and cleanup are distinct gates.
The implementation plan will name each gate and its verification. No external
state changes occur merely from approving this design or its plan.

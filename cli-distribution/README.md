# mPaka CLI distribution Worker

This Worker serves the public distribution paths from the private
`mpaka-cli-releases` R2 bucket. It has no listing endpoint and only permits the
installer scripts, `latest`, signed manifests, and the six approved artifacts.

Install dependencies with `pnpm install`, run `pnpm test`, and validate the
Wrangler configuration with `pnpm deploy:dry-run`. Production credentials must
come from Cloudflare's environment or `CLOUDFLARE_API_TOKEN`; never commit
them. Cloudflare Access must allow anonymous access only to `cli.mpaka.dev`; the
landing root remains protected by the existing Access application.

# mPaka CLI

`mpaka` is the command-line client for the [mPaka](https://mpaka.dev) code
execution API. It handles account registration and login, submits code for
remote execution, lists past executions, and updates itself from signed
releases.

The CLI never runs submitted code locally: it only talks to the mPaka API.

## Install

Linux and macOS:

```bash
curl -fsSL https://cli.mpaka.dev/install | sh
```

Windows PowerShell:

```powershell
irm https://cli.mpaka.dev/install.ps1 | iex
```

Installers verify the signed release manifest and the SHA-256 hash of every
archive before installing. See [docs/cli.md](docs/cli.md) for supported
targets, pinned versions, self-update, and usage.

## Build from source

Requires stable Rust (edition 2024).

```bash
cargo build --release --bin mpaka
./target/release/mpaka --help
```

## Development

```bash
make check   # formatting, tests, Clippy, release, installer, and doc contracts
```

Tests use [`cargo nextest`](https://nexte.st/). The release and installer checks
also need `openssl`, `shellcheck`, and optionally `pwsh`.

## Releases

Pushing an annotated `vMAJOR.MINOR.PATCH` tag that matches the version in
`Cargo.toml` builds all six targets, signs the manifest, and publishes it to
`https://cli.mpaka.dev`. Published versions are immutable. See
[docs/release.md](docs/release.md) for the Cloudflare and GitHub setup,
rollback, and key rotation.

## License

[MIT](LICENSE)

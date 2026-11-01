# Retired release key fixture

`manifest.json` and `manifest.json.sig` are the published v0.2.0 bridge
manifest and its detached signature, made with the retired RSA-3072 release
key. `public.pem` is that retired key's public half (SPKI SHA-256
`fec4af38209410541558e098f79a11eb3a405512ea30a0f8b7106735623296e0`).

Tests first prove the signature is valid under `public.pem`, then assert that
the CLI and both installers reject it, because v0.3.0 and later trust only the
new release key. These files are public data; no private key is involved.

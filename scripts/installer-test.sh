#!/usr/bin/env bash
set -Eeuo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="$(mktemp -d)"
server_pid=''
cleanup() {
  if [[ -n "$server_pid" ]]; then kill "$server_pid" 2>/dev/null || true; fi
  rm -rf "$work"
}
trap cleanup EXIT

input="$work/input"
artifacts="$work/artifacts"
release="$work/release"
server_root="$work/server"
mkdir -p "$input" "$artifacts" "$release" "$server_root/cli/releases/v0.1.0"

for target in \
  x86_64-unknown-linux-musl \
  armv7-unknown-linux-musleabihf \
  aarch64-unknown-linux-musl \
  x86_64-pc-windows-msvc \
  x86_64-apple-darwin \
  aarch64-apple-darwin; do
  printf '#!/bin/sh\nprintf "mpaka 0.1.0\\n"\n' > "$input/mpaka-$target"
done

signing_key="$work/signing-key.pem"
public_key="$work/public.pem"
openssl genrsa 2048 2>/dev/null > "$signing_key"
openssl rsa -in "$signing_key" -pubout -out "$public_key" 2>/dev/null
"$root/scripts/cli-release.sh" package --version v0.1.0 --input-dir "$input" --output-dir "$artifacts"
"$root/scripts/cli-release.sh" manifest --version v0.1.0 --artifact-dir "$artifacts" --output-dir "$release" --signing-key "$signing_key"
cp "$release"/* "$server_root/cli/releases/v0.1.0/"
printf 'v0.1.0\n' > "$server_root/cli/latest"

cert="$work/cert.pem"
openssl req -x509 -newkey rsa:2048 -nodes -keyout "$work/tls-key.pem" -out "$cert" -days 1 -subj '/CN=localhost' 2>/dev/null
port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
(cd "$server_root" && openssl s_server -quiet -WWW -accept "$port" -cert "$cert" -key "$work/tls-key.pem" >"$work/server.log" 2>&1) &
server_pid=$!
for _ in {1..50}; do
  if curl --silent --show-error --fail --cacert "$cert" "https://localhost:$port/cli/latest" >/dev/null 2>&1; then break; fi
  sleep 0.1
done

base_url="https://localhost:$port/cli"
public_pem="$(cat "$public_key")"

for pair in 'Linux:x86_64' 'Linux:armv7l' 'Linux:aarch64' 'Darwin:x86_64' 'Darwin:arm64'; do
  os="${pair%%:*}"; arch="${pair##*:}"
  target_output="$(MPAKA_CLI_BASE_URL="$base_url" MPAKA_CLI_TEST_ONLY_TARGET=1 MPAKA_CLI_TEST_UNAME_S="$os" MPAKA_CLI_TEST_UNAME_M="$arch" "$root/install" 2>&1)"
  [[ "$target_output" == *"target="* ]] || { printf 'target detection failed for %s/%s\n' "$os" "$arch" >&2; exit 1; }
done

home="$work/home"
install_output="$(HOME="$home" MPAKA_CLI_BASE_URL="$base_url" MPAKA_CLI_CA_CERT="$cert" MPAKA_CLI_TEST_PUBLIC_KEY_PEM="$public_pem" "$root/install" 2>&1)"
test -x "$home/.local/bin/mpaka"
test "$("$home/.local/bin/mpaka" --version)" = 'mpaka 0.1.0'
[[ "$install_output" == *"PATH"* ]]

if MPAKA_CLI_BASE_URL='http://127.0.0.1:1/cli' "$root/install" >/dev/null 2>&1; then
  printf 'installer accepted a non-HTTPS base URL\n' >&2
  exit 1
fi
if MPAKA_CLI_BASE_URL="$base_url" MPAKA_CLI_CA_CERT="$cert" MPAKA_CLI_TEST_PUBLIC_KEY_PEM="$(cat "$work/tls-key.pem")" HOME="$work/bad-home" "$root/install" >/dev/null 2>&1; then
  printf 'installer accepted an invalid manifest signature\n' >&2
  exit 1
fi
if MPAKA_CLI_BASE_URL="$base_url" MPAKA_CLI_TEST_ONLY_TARGET=1 MPAKA_CLI_TEST_UNAME_S='FreeBSD' MPAKA_CLI_TEST_UNAME_M='sparc64' "$root/install" >/dev/null 2>&1; then
  printf 'installer accepted an unknown platform\n' >&2
  exit 1
fi

printf 'posix installer fixture tests passed\n'

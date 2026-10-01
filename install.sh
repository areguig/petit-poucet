#!/bin/sh
# Installs the petit-poucet binary: the latest release, or PETIT_POUCET_VERSION. Re-run it to upgrade.
#   curl -fsSL https://raw.githubusercontent.com/areguig/petit-poucet/main/install.sh | sh
set -eu

log() { printf 'petit-poucet install: %s\n' "$*" >&2; }
fail() { log "$*"; exit 1; }

case "$(uname -s)-$(uname -m)" in
  Darwin-arm64) target=aarch64-apple-darwin ;;
  Darwin-x86_64) target=x86_64-apple-darwin ;;
  Linux-x86_64) target=x86_64-unknown-linux-musl ;;
  Linux-aarch64 | Linux-arm64) target=aarch64-unknown-linux-musl ;;
  *) fail "no release for $(uname -s) $(uname -m)" ;;
esac

releases="${PETIT_POUCET_RELEASES:-https://github.com/areguig/petit-poucet/releases}"
if [ -n "${PETIT_POUCET_VERSION:-}" ]; then
  url="$releases/download/v${PETIT_POUCET_VERSION#v}/petit-poucet-$target"
else
  url="$releases/latest/download/petit-poucet-$target"
fi
dir="${PETIT_POUCET_INSTALL_DIR:-$HOME/.local/bin}"

sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$dir"
tmp="$dir/.petit-poucet.download.$$"
trap 'rm -f "$tmp"' EXIT
log "downloading $url"
curl --fail --silent --show-error --location --connect-timeout 15 --max-time 300 --output "$tmp" "$url" \
  || fail "download failed: $url"
expected=$(curl --fail --silent --show-error --location --connect-timeout 15 "$url.sha256" | cut -d' ' -f1) \
  || fail "checksum download failed: $url.sha256"
actual=$(sha256_of "$tmp")
[ -n "$expected" ] && [ "$actual" = "$expected" ] || fail "checksum mismatch for $url (expected $expected, got $actual)"
chmod +x "$tmp"
mv -f "$tmp" "$dir/petit-poucet"
trap - EXIT

log "installed $("$dir/petit-poucet" --version) in $dir"
case ":$PATH:" in
  *":$dir:"*) ;;
  *) log "$dir is not on your PATH: add it to your shell profile" ;;
esac
log "next: run \`petit-poucet init\` to create your memory vault"

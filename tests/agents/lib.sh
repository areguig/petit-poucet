# Shared by the agent scripts: run from the repo root after `cargo build --release`.
set -eu

pp="$PWD/target/release/petit-poucet"
[ -x "$pp" ] || { echo "build first: cargo build --release" >&2; exit 1; }
repo="$PWD"
# A throwaway HOME, so a local run never touches the real agents' config or vault.
HOME=$(mktemp -d)
export HOME

fail() { echo "FAIL: $*" >&2; exit 1; }
# has <text> <pattern>: one line of text matches the grep pattern.
has() { printf '%s\n' "$1" | grep -q -- "$2" || fail "expected /$2/ in:
$1"; }

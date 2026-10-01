#!/bin/sh
# Installs petit-poucet into the real Codex CLI with `setup`, and checks Codex reads it.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.codex"
has "$("$pp" setup)" '^Codex: set up'
has "$(codex mcp get petit-poucet 2>/dev/null)" "command: $pp"
has "$(codex mcp list 2>/dev/null)" '^petit-poucet .* enabled'
has "$("$pp" setup --check)" '^Codex: set up'

has "$("$pp" setup --uninstall)" '^Codex: removed'
if codex mcp get petit-poucet >/dev/null 2>&1; then fail "Codex still lists petit-poucet"; fi
echo "codex: ok"

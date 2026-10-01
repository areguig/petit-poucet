#!/bin/sh
# Installs petit-poucet into the real Cursor CLI with `setup`, and checks Cursor starts the server and lists its tools.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.cursor"
has "$("$pp" setup)" '^Cursor: set up'
has "$(cursor-agent mcp list 2>&1)" '^petit-poucet: ready'
has "$(cursor-agent mcp list-tools petit-poucet 2>&1)" '^- memory_index '
has "$("$pp" setup --check)" '^Cursor: set up'

has "$("$pp" setup --uninstall)" '^Cursor: removed'
if cursor-agent mcp list 2>&1 | grep -q petit-poucet; then fail "Cursor still lists petit-poucet"; fi
echo "cursor: ok"

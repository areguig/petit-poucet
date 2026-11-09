#!/bin/sh
# Installs petit-poucet into the real Cursor CLI with `setup`, and checks Cursor starts the server and lists its tools.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.cursor"
# Cursor's Windows installer adds `cursor-agent.cmd`, which Git Bash only runs by its full name.
cursor=cursor-agent
[ -z "$windows" ] || cursor=cursor-agent.cmd
has "$("$pp" setup)" '^Cursor: set up'
doctor
has "$("$cursor" mcp list 2>&1)" '^petit-poucet: ready'
# Unverified: this script has no model session, so how Cursor delivers the session view to the model is not checked.
has "$("$cursor" mcp list-tools petit-poucet 2>&1)" '^- memory_index '
has "$("$pp" setup --check)" '^Cursor: set up'

has "$("$pp" setup --uninstall)" '^Cursor: removed'
if "$cursor" mcp list 2>&1 | grep -q petit-poucet; then fail "Cursor still lists petit-poucet"; fi
echo "cursor: ok"

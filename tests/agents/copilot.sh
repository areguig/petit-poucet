#!/bin/sh
# Installs the plugin from this checkout into the real Copilot CLI, and checks Copilot lists its MCP server.
. "$(dirname "$0")/lib.sh"

"$pp" setup >/dev/null
copilot plugin marketplace add "$repo" >/dev/null
copilot plugin install petit-poucet@petit-poucet >/dev/null
has "$(copilot mcp list 2>&1)" 'petit-poucet (local)'
has "$("$pp" setup --check)" '^GitHub Copilot: set up by its plugin'

uninstall=$("$pp" setup --uninstall | sed -n 's/^GitHub Copilot: remove the plugin with `\(.*\)`$/\1/p')
[ -n "$uninstall" ] || fail "setup --uninstall gave no command for Copilot"
sh -c "$uninstall" >/dev/null
if "$pp" setup --check >/dev/null; then fail "check still passes after the plugin was removed"; fi
echo "copilot: ok"

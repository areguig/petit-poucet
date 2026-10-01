#!/bin/sh
# Installs the plugin from this checkout into the real Claude Code, and checks its MCP server connects.
. "$(dirname "$0")/lib.sh"

"$pp" setup >/dev/null
claude plugin marketplace add "$repo" >/dev/null
claude plugin install petit-poucet@petit-poucet >/dev/null
has "$(claude mcp list 2>&1)" 'petit-poucet.*Connected'
has "$("$pp" setup --check)" '^Claude Code: set up by its plugin'

uninstall=$("$pp" setup --uninstall | sed -n 's/^Claude Code: remove the plugin with `\(.*\)`$/\1/p')
[ -n "$uninstall" ] || fail "setup --uninstall gave no command for Claude Code"
sh -c "$uninstall" >/dev/null
if "$pp" setup --check >/dev/null; then fail "check still passes after the plugin was removed"; fi
echo "claude: ok"

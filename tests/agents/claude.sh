#!/bin/sh
# Installs the plugin from this checkout into the real Claude Code, then runs a session against the mock model:
# the plugin's hooks, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

# The plugin's launcher prefers a petit-poucet of its pinned version on PATH: this build.
PATH="$repo/target/release:$PATH"
"$pp" setup >/dev/null
claude plugin marketplace add "$repo" >/dev/null
claude plugin install petit-poucet@petit-poucet >/dev/null
has "$(claude mcp list 2>&1)" 'petit-poucet.*Connected'
has "$("$pp" setup --check)" '^Claude Code: set up by its plugin'

start_model
cd "$(mktemp -d)"
resume=
for turn in one two three; do
  ANTHROPIC_BASE_URL="$model_url" ANTHROPIC_API_KEY=dummy claude -p $resume "$turn" </dev/null >/dev/null
  resume=--continue
done
cd "$repo"
check_calls mcp__plugin_petit-poucet_petit-poucet__memory_search

uninstall=$("$pp" setup --uninstall | sed -n 's/^Claude Code: remove the plugin with `\(.*\)`$/\1/p')
[ -n "$uninstall" ] || fail "setup --uninstall gave no command for Claude Code"
sh -c "$uninstall" >/dev/null
if "$pp" setup --check >/dev/null; then fail "check still passes after the plugin was removed"; fi
echo "claude: ok"

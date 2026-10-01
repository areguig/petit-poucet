#!/bin/sh
# Installs the plugin from this checkout into the real Copilot CLI, then runs a session against the mock model:
# the plugin's hooks, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

# The plugin's launcher prefers a petit-poucet of its pinned version on PATH: this build.
PATH="$repo/target/release:$PATH"
"$pp" setup >/dev/null
copilot plugin marketplace add "$repo" >/dev/null
copilot plugin install petit-poucet@petit-poucet >/dev/null
has "$(copilot mcp list 2>&1)" 'petit-poucet (local)'
has "$("$pp" setup --check)" '^GitHub Copilot: set up by its plugin'

# Copilot CLI uses another model provider with BYOK, no GitHub sign-in needed.
start_model
cd "$(mktemp -d)"
resume=
for turn in one two three; do
  COPILOT_PROVIDER_BASE_URL="$model_url/v1" COPILOT_PROVIDER_API_KEY=dummy COPILOT_MODEL=mock \
    copilot -p "$turn" $resume --allow-all-tools </dev/null >/dev/null
  resume=--continue
done
cd "$repo"
check_calls petit-poucet-memory_search

uninstall=$("$pp" setup --uninstall | sed -n 's/^GitHub Copilot: remove the plugin with `\(.*\)`$/\1/p')
[ -n "$uninstall" ] || fail "setup --uninstall gave no command for Copilot"
sh -c "$uninstall" >/dev/null
if "$pp" setup --check >/dev/null; then fail "check still passes after the plugin was removed"; fi
echo "copilot: ok"

#!/bin/sh
# Installs petit-poucet into the real Copilot CLI with `setup`, replacing the old plugin, then runs a session
# against the mock model: its hooks, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.copilot"

install_old_plugin copilot
out=$("$pp" setup)
has "$out" '^GitHub Copilot: set up'
# setup replaces the old plugin where there is one.
if [ -z "$windows" ]; then
  has "$out" '^GitHub Copilot: removed its old petit-poucet plugin'
  # A plugin loaded from a local folder is disabled rather than deleted.
  if copilot plugin list 2>&1 | grep -q 'petit-poucet@petit-poucet.*(enabled)'; then fail "the old plugin is still enabled"; fi
fi
has "$(copilot mcp list 2>&1)" 'petit-poucet'
has "$("$pp" setup --check)" '^GitHub Copilot skills: installed'

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

has "$("$pp" setup --uninstall)" '^GitHub Copilot: removed'
if copilot mcp list 2>&1 | grep -q petit-poucet; then fail "Copilot still lists petit-poucet"; fi
echo "copilot: ok"

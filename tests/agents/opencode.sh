#!/bin/sh
# Installs petit-poucet into the real OpenCode with `setup`, then runs a session against the mock model:
# its plugin, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

# OpenCode follows XDG; setup writes ~/.config/opencode, where XDG points by default.
unset XDG_CONFIG_HOME
export OPENCODE_DISABLE_AUTOUPDATE=1 OPENCODE_DISABLE_MODELS_FETCH=1
# OpenCode's Code Mode offers MCP tools through its `execute` tool, which runs code calling them.
export REVIEW_TOOL=execute REVIEW_ARGS='{"code": "return await tools[\"petit-poucet\"].memory_review({})"}'
start_model
mkdir -p "$HOME/.config/opencode"
cat > "$HOME/.config/opencode/opencode.json" <<JSON
{
  "model": "mock/mock",
  "share": "disabled",
  "providers": {"mock": {
    "package": "@opencode/ai/providers/openai-compatible",
    "settings": {"baseURL": "$model_url/v1", "apiKey": "dummy"},
    "models": {"mock": {"name": "mock"}}
  }}
}
JSON
has "$("$pp" setup)" '^OpenCode: set up'
# OpenCode's background service starts here, with this script's environment, and outlives each `opencode run`:
# the reminder the plugin sends once a turn ends still runs.
trap 'opencode service stop >/dev/null 2>&1; kill $model_pid' EXIT
# OpenCode connects MCP servers per folder once a session opens there: a first session in this folder does it.
work=$(mktemp -d)
cd "$work"
opencode run warm-up </dev/null >/dev/null
: > "$calls"
continue=
for turn in one two three; do
  opencode run $continue "$turn" </dev/null >/dev/null
  continue=--continue
done
cd "$repo"
# The reminder's own turn, after the third.
tries=0
until grep -q 'Memory check' "$calls"; do
  tries=$((tries + 1))
  [ "$tries" -le 100 ] || break
  sleep 0.2
done
# Code Mode lists the tools in a catalog; the server's instructions come with them.
check_calls "petit-poucet holds the user's memory"
check_update

big_vault
cd "$work"
# opencode run can't ask for approval: --auto approves the tool call, as a user would.
opencode run --auto petit-poucet-review </dev/null >/dev/null
cd "$repo"
check_review

has "$("$pp" setup --check)" '^OpenCode: set up'

has "$("$pp" setup --uninstall)" '^OpenCode: removed'
if opencode mcp list 2>&1 | grep -q petit-poucet; then fail "OpenCode still lists petit-poucet"; fi
echo "opencode: ok"

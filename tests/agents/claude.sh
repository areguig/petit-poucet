#!/bin/sh
# Installs petit-poucet into the real Claude Code with `setup`, replacing the old plugin, then runs a session
# against the mock model: its hooks, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.claude"

install_old_plugin claude
out=$("$pp" setup)
has "$out" '^Claude Code: set up'
# setup replaces the old plugin where there is one.
if [ -z "$windows" ]; then
  has "$out" '^Claude Code: removed its old petit-poucet plugin'
  if claude plugin list 2>&1 | grep -q 'petit-poucet@'; then fail "the old plugin is still installed"; fi
fi
has "$(claude mcp list 2>&1)" '^petit-poucet: .*petit-poucet.* serve .*Connected'
has "$("$pp" setup --check)" '^Claude Code skills: installed'

export REVIEW_TOOL=mcp__petit-poucet__memory_review
start_model
cd "$(mktemp -d)"
resume=
for turn in one two three; do
  ANTHROPIC_BASE_URL="$model_url" ANTHROPIC_API_KEY=dummy claude -p $resume "$turn" </dev/null >/dev/null
  resume=--continue
done
cd "$repo"
check_calls mcp__petit-poucet__memory_search
check_update

big_vault
cd "$(mktemp -d)"
ANTHROPIC_BASE_URL="$model_url" ANTHROPIC_API_KEY=dummy claude -p petit-poucet-review \
  --allowedTools mcp__petit-poucet__memory_review </dev/null >/dev/null
cd "$repo"
check_review

has "$("$pp" setup --uninstall)" '^Claude Code: removed'
if claude mcp list 2>&1 | grep -q petit-poucet; then fail "Claude Code still lists petit-poucet"; fi
echo "claude: ok"

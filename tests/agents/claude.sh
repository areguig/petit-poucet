#!/bin/sh
# Installs petit-poucet into the real Claude Code with `setup`, replacing the old plugin, then runs a session
# against the mock model: its hooks, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

# petit-poucet 0.2 came as this plugin: setup removes it first.
PATH="$repo/target/release:$PATH"
claude plugin marketplace add "$repo" >/dev/null
claude plugin install petit-poucet@petit-poucet >/dev/null
out=$("$pp" setup)
has "$out" '^Claude Code: removed its old petit-poucet plugin'
has "$out" '^Claude Code: set up'
if claude plugin list 2>&1 | grep -q 'petit-poucet@'; then fail "the old plugin is still installed"; fi
has "$(claude mcp list 2>&1)" "^petit-poucet: $pp serve .*Connected"
has "$("$pp" setup --check)" '^Claude Code skills: installed'

start_model
cd "$(mktemp -d)"
resume=
for turn in one two three; do
  ANTHROPIC_BASE_URL="$model_url" ANTHROPIC_API_KEY=dummy claude -p $resume "$turn" </dev/null >/dev/null
  resume=--continue
done
cd "$repo"
check_calls mcp__petit-poucet__memory_search

has "$("$pp" setup --uninstall)" '^Claude Code: removed'
if claude mcp list 2>&1 | grep -q petit-poucet; then fail "Claude Code still lists petit-poucet"; fi
echo "claude: ok"

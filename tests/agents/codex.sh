#!/bin/sh
# Installs petit-poucet into the real Codex CLI with `setup`, then runs a session against the mock model:
# its hooks, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

export REVIEW_TOOL=memory_review REVIEW_NAMESPACE=mcp__petit_poucet
node --test "$repo/tests/agents/calls.test.mjs"
start_model
# The mock as Codex's model provider; top-level keys go before the tables `setup` adds.
mkdir -p "$HOME/.codex"
cat > "$HOME/.codex/config.toml" <<TOML
model = "mock"
model_provider = "mock"

[model_providers.mock]
name = "mock"
base_url = "$model_url/v1"
wire_api = "responses"
env_key = "MOCK_API_KEY"
TOML
has "$("$pp" setup)" '^Codex: set up'
has "$(codex mcp get petit-poucet 2>&1)" 'command: .*petit-poucet'
has "$(codex mcp list 2>/dev/null)" '^petit-poucet .* enabled'

# Users trust the hooks once in Codex's /hooks; `codex exec` would skip untrusted ones silently.
session_dir=$(mktemp -d)
if [ -n "$windows" ]; then
  # Codex finds the user-level skills in the OS-resolved profile, which the throwaway HOME doesn't redirect:
  # making that home a project root gives it the same skills as the project's .agents/skills.
  git init -q "$HOME"
  session_dir="$HOME"
fi
cd "$session_dir"
codex="codex exec --dangerously-bypass-hook-trust --skip-git-repo-check"
MOCK_API_KEY=dummy $codex one </dev/null >/dev/null
for turn in two three; do
  MOCK_API_KEY=dummy codex exec resume --last --dangerously-bypass-hook-trust --skip-git-repo-check "$turn" </dev/null >/dev/null
done
cd "$repo"
# The skills catalog Codex sent the model lists the three installed skills (#159).
node "$repo/tests/agents/calls.mjs" "$calls" x --catalog=memory,migrate-memory,tidy-memory "--codex-version=$(codex --version 2>/dev/null | head -n 1)" ||
  fail "Codex's skills catalog lacks an installed skill"
# Codex offers MCP tools through its tool search, which lists each server with its instructions.
check_calls "petit-poucet holds the user's memory"
check_update

big_vault
# codex exec can't ask for approval, and MCP calls need it: approve this one tool, as a user would.
printf '\n[mcp_servers.petit-poucet.tools.memory_review]\napproval_mode = "approve"\n' >> "$HOME/.codex/config.toml"
cd "$(mktemp -d)"
MOCK_API_KEY=dummy $codex petit-poucet-review </dev/null >/dev/null
cd "$repo"
check_review

has "$("$pp" setup --check)" '^Codex: set up'

has "$("$pp" setup --uninstall)" '^Codex: removed'
if codex mcp get petit-poucet >/dev/null 2>&1; then fail "Codex still lists petit-poucet"; fi
echo "codex: ok"

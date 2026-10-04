#!/bin/sh
# Installs petit-poucet into the real Antigravity CLI with `setup`, then runs a session against the mock model:
# its hooks, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.gemini/config"
has "$("$pp" setup)" '^Antigravity CLI: set up'
has "$(agy mcp list 2>&1)" '^petit-poucet .*enabled'
# A subagent only: not one of the agents a session runs as.
if agy agents 2>&1 | grep -q memory-cleanup; then fail "memory-cleanup is offered as a main agent"; fi

# agy needs a Google sign-in, except through an LLM gateway: the mock model stands in for one.
# agy reaches MCP tools through call_mcp_tool.
export REVIEW_TOOL=call_mcp_tool
export REVIEW_ARGS='{"ServerName":"petit-poucet","ToolName":"memory_review","Arguments":{},"toolSummary":"Memory review","toolAction":"Reviewing memory"}'
start_model
cd "$(mktemp -d)"
session=$(for turn in one two three; do printf '{"event":"user","message":{"content":"%s"}}\n' "$turn"; done |
  AGY_LLM_GATEWAY_URL="$model_url" AGY_LLM_GATEWAY_API_KEY=dummy \
  agy --input-format stream-json --output-format stream-json -p="")
has "$session" '"event":"result".*"status":"SUCCESS"'
cd "$repo"
# agy names each MCP server in its system prompt, above the server's tools.
check_calls "# petit-poucet"
check_update

big_vault
# Headless agy denies MCP calls it can't ask about: allow this one tool, as a user would.
mkdir -p "$HOME/.gemini/antigravity-cli"
echo '{"permissions": {"allow": ["mcp(petit-poucet/memory_review)"]}}' > "$HOME/.gemini/antigravity-cli/settings.json"
cd "$(mktemp -d)"
printf '{"event":"user","message":{"content":"petit-poucet-review"}}\n' |
  AGY_LLM_GATEWAY_URL="$model_url" AGY_LLM_GATEWAY_API_KEY=dummy \
  agy --input-format stream-json --output-format stream-json -p="" >/dev/null
cd "$repo"
check_review

has "$("$pp" setup --check)" '^Antigravity CLI: set up'

has "$("$pp" setup --uninstall)" '^Antigravity CLI: removed'
if agy mcp list 2>&1 | grep -q petit-poucet; then fail "Antigravity still lists petit-poucet"; fi
echo "antigravity: ok"

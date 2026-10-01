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
start_model
cd "$(mktemp -d)"
session=$(for turn in one two three; do printf '{"event":"user","message":{"content":"%s"}}\n' "$turn"; done |
  AGY_LLM_GATEWAY_URL="$model_url" AGY_LLM_GATEWAY_API_KEY=dummy \
  agy --input-format stream-json --output-format stream-json -p="")
has "$session" '"event":"result".*"status":"SUCCESS"'
cd "$repo"
# agy names each MCP server in its system prompt, above the server's tools.
check_calls "# petit-poucet"
has "$("$pp" setup --check)" '^Antigravity CLI: set up'

has "$("$pp" setup --uninstall)" '^Antigravity CLI: removed'
if agy mcp list 2>&1 | grep -q petit-poucet; then fail "Antigravity still lists petit-poucet"; fi
echo "antigravity: ok"

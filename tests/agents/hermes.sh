#!/bin/sh
# Installs petit-poucet into the real Hermes Agent with `setup`, then runs a session against the mock model:
# its hook, MCP server and skills all reach the model, and the user's config keeps its comments.
. "$(dirname "$0")/lib.sh"

# lib.sh points HERMES_HOME here, which Hermes and setup both follow.
config="$HOME/.hermes/config.yaml"
# Hermes lists MCP tools in a catalog and calls them through its `tool_call` tool.
export REVIEW_TOOL=tool_call REVIEW_ARGS='{"name": "mcp__petit_poucet__memory_review", "arguments": {}}'
export READ_TOOL=tool_call READ_ARGS='{"name": "mcp__petit_poucet__memory_read", "arguments": {"path": "{name}"}}'
export SAVE_TOOL=tool_call SAVE_ARGS="{\"name\": \"mcp__petit_poucet__memory_save\", \"arguments\": $preference_json}"
start_model
mkdir -p "$HOME/.hermes"
cat > "$config" <<YAML
# The user's own comment.
model:
  provider: custom
  default: mock
  base_url: $model_url/v1
YAML
export OPENAI_API_KEY=dummy
has "$("$pp" setup)" '^Hermes Agent: set up'
grep -q "^# The user's own comment.$" "$config" || fail "setup lost the user's comment"
doctor
oversize_memory hermes

work=$(mktemp -d)
cd "$work"
: > "$calls"
continue=
for turn in one two three; do
  # --accept-hooks approves petit-poucet's hook, as the user does once at Hermes's prompt.
  hermes chat --accept-hooks -Q $continue -q "$turn" </dev/null >/dev/null
  continue=--continue
done
cd "$repo"
# Hermes has no hook after an answer, where the save reminder belongs, and no tidy-memory, whose subagent it can't take.
check_calls "mcp__petit_poucet__memory_save" --no-reminder --skill=migrate-memory
check_update

big_vault
cd "$work"
hermes chat --accept-hooks -Q -q petit-poucet-review </dev/null >/dev/null
cd "$repo"
check_review

probe_note hermes
cd "$work"
hermes chat --accept-hooks -Q -q petit-poucet-read </dev/null >/dev/null
cd "$repo"
check_read

before_save
cd "$work"
hermes chat --accept-hooks -Q -q petit-poucet-save </dev/null >/dev/null
cd "$repo"
check_save

has "$("$pp" setup --check)" '^Hermes Agent: set up'
has "$("$pp" setup --uninstall)" '^Hermes Agent: removed'
if grep -q petit-poucet "$config"; then fail "Hermes's config still names petit-poucet"; fi
grep -q "^# The user's own comment.$" "$config" || fail "uninstall lost the user's comment"
echo "hermes: ok"

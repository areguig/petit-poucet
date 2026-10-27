#!/bin/sh
# Installs petit-poucet into the real Hermes Agent with `setup`, then runs a session against the mock model:
# its hook, MCP server and skills all reach the model, and the user's config keeps its comments.
. "$(dirname "$0")/lib.sh"

# lib.sh points HERMES_HOME here, which Hermes and setup both follow.
config="$HOME/.hermes/config.yaml"
# Hermes lists MCP tools in a catalog and calls them through its `tool_call` tool.
export REVIEW_TOOL=tool_call REVIEW_ARGS='{"name": "mcp__petit_poucet__memory_review", "arguments": {}}'
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
# More memory than Hermes passes on whole by default (10,000 characters): setup makes room for it.
i=0
while [ "$i" -lt 90 ]; do
  printf -- '---\ntype: user\nscope: all repos\nsummary: preference %s, worded like a real one-line summary of a rule the user stated about how they work\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n**Why:** x\n' "$i" > "$HOME/agent-memory/Preferences/preference-$i.md"
  i=$((i + 1))
done

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
grep -q "preference 89, worded" "$calls" || fail "the memory was cut short"
check_update

big_vault
cd "$work"
hermes chat --accept-hooks -Q -q petit-poucet-review </dev/null >/dev/null
cd "$repo"
check_review

has "$("$pp" setup --check)" '^Hermes Agent: set up'
has "$("$pp" setup --uninstall)" '^Hermes Agent: removed'
if grep -q petit-poucet "$config"; then fail "Hermes's config still names petit-poucet"; fi
grep -q "^# The user's own comment.$" "$config" || fail "uninstall lost the user's comment"
echo "hermes: ok"

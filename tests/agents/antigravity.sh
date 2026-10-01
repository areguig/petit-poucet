#!/bin/sh
# Installs petit-poucet into the real Antigravity CLI with `setup`, then runs an agy session against a fake model:
# every model call carries the memory once, the MCP tools are offered, and the stop reminder comes once without looping.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.gemini/config"
has "$("$pp" setup)" '^Antigravity CLI: set up'
has "$(agy mcp list 2>&1)" '^petit-poucet .*enabled'

# agy needs a Google sign-in, except through an LLM gateway: the fake model stands in for one.
requests="$HOME/requests.jsonl"
python3 "$repo/tests/agents/fake-gemini.py" "$requests" > "$HOME/port" &
model=$!
trap 'kill $model' EXIT
while [ ! -s "$HOME/port" ]; do sleep 0.1; done

# Three turns: the third stop of a session gets the save reminder.
cd "$(mktemp -d)"
session=$(for turn in one two three; do printf '{"event":"user","message":{"content":"%s"}}\n' "$turn"; done |
  AGY_LLM_GATEWAY_URL="http://127.0.0.1:$(cat "$HOME/port")" AGY_LLM_GATEWAY_API_KEY=dummy \
  agy --input-format stream-json --output-format stream-json -p="")
has "$session" '"event":"result".*"status":"SUCCESS"'

python3 - "$requests" <<'EOF' || fail "the model calls above don't carry petit-poucet as expected"
import json, sys
calls = [json.loads(line) for line in open(sys.argv[1])]
# The first call of a session only names the conversation.
calls = [c for c in calls if "title generator" not in json.dumps(c["systemInstruction"])]
texts = lambda call: [p.get("text", "") for c in call["contents"] for p in c["parts"]]
for call in calls:
    memory = [t for t in texts(call) if t.startswith("Agent memory (petit-poucet)")]
    system = json.dumps(call["systemInstruction"])
    print(len(memory), "memory message(s);", "petit-poucet tools offered:", "# petit-poucet" in system and "memory_search" in system)
    assert len(memory) == 1 and "# petit-poucet" in system and "memory_search" in system
reminded = [any("Memory check" in t for t in texts(call)) for call in calls]
print("reminder in calls:", reminded)
assert reminded == [False] * 3 + [True], "one reminder, after the third turn, and no loop"
EOF
cd "$repo"
has "$("$pp" setup --check)" '^Antigravity CLI: set up'

has "$("$pp" setup --uninstall)" '^Antigravity CLI: removed'
if agy mcp list 2>&1 | grep -q petit-poucet; then fail "Antigravity still lists petit-poucet"; fi
echo "antigravity: ok"

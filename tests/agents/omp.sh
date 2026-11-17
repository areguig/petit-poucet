#!/bin/sh
# Installs petit-poucet into the real omp with `setup`, then runs a session against the mock model:
# its extension, MCP server and skills all reach the model.
. "$(dirname "$0")/lib.sh"

export REVIEW_TOOL=mcp__petit_poucet_memory_review
export READ_TOOL=mcp__petit_poucet_memory_read READ_ARGS='{"path": "{name}"}'
export SAVE_TOOL=mcp__petit_poucet_memory_save SAVE_ARGS="$preference_json"
start_model
mkdir -p "$HOME/.omp/agent"
cat > "$HOME/.omp/agent/models.yml" <<YAML
providers:
  mock:
    baseUrl: $model_url/v1
    apiKey: dummy
    api: openai-completions
    models: [{id: mock, name: mock}]
YAML
has "$("$pp" setup)" '^omp: set up'
doctor
oversize_memory omp
# A subagent of its own folder: omp skips ~/.claude/agents.
[ -f "$HOME/.omp/agent/agents/memory-cleanup.md" ] || fail "no memory-cleanup subagent for omp"

cd "$(mktemp -d)"
# Each message is a turn of the same session; the reminder the extension sends after the third is a fourth.
omp -p --model mock/mock --no-title one two three </dev/null >/dev/null
cd "$repo"
check_calls "mcp__petit_poucet_memory_save"
check_update

big_vault
cd "$(mktemp -d)"
omp -p --model mock/mock --no-title --approval-mode yolo petit-poucet-review </dev/null >/dev/null
cd "$repo"
check_review

probe_note omp
cd "$(mktemp -d)"
omp -p --model mock/mock --no-title --approval-mode yolo petit-poucet-read </dev/null >/dev/null
cd "$repo"
check_read

before_save
cd "$(mktemp -d)"
omp -p --model mock/mock --no-title --approval-mode yolo petit-poucet-save </dev/null >/dev/null
cd "$repo"
check_save

has "$("$pp" setup --check)" '^omp: set up'

has "$("$pp" setup --uninstall)" '^omp: removed'
[ ! -f "$HOME/.omp/agent/extensions/petit-poucet.js" ] || fail "the extension is still there"
echo "omp: ok"

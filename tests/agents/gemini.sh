#!/bin/sh
# Installs petit-poucet into the real Gemini CLI with `setup`, and checks Gemini connects to it and runs its session-start hook.
. "$(dirname "$0")/lib.sh"

mkdir -p "$HOME/.gemini"
has "$("$pp" setup)" '^Gemini CLI: set up'

# Gemini CLI starts MCP servers only in trusted folders; it compares the physical path (macOS temp is a symlink).
work=$(cd "$(mktemp -d)" && pwd -P)
printf '{"%s": "TRUST_FOLDER"}\n' "$work" > "$HOME/.gemini/trustedFolders.json"
cd "$work"
has "$(gemini mcp list 2>&1)" 'petit-poucet: .*Connected'
# A dummy key gets the session started, so the session-start hook runs; only the model call fails after it.
has "$(GEMINI_API_KEY=dummy gemini -p hi 2>&1 || true)" '[0-9] notes loaded ('
grep -rqs "Agent memory (petit-poucet)" "$HOME/.gemini/tmp" || fail "the session-start context isn't in Gemini's transcript"
cd "$repo"
has "$("$pp" setup --check)" '^Gemini CLI: set up'

has "$("$pp" setup --uninstall)" '^Gemini CLI: removed'
cd "$work"
if gemini mcp list 2>&1 | grep -q petit-poucet; then fail "Gemini still lists petit-poucet"; fi
echo "gemini: ok"

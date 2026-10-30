#!/bin/sh
# Runs one agent's script with every agent folder variable naming a profile of the user's, and the vault
# variable naming a vault of theirs, then checks the script left those as they were: the tests stay in their throwaway HOME.
# Usage: sh tests/agents/isolated.sh <agent>
set -eu
outside=$(mktemp -d)
for var in COPILOT_HOME CODEX_HOME HERMES_HOME; do
  mkdir -p "$outside/$var/skills/memory"
  echo "the user's skill" > "$outside/$var/skills/memory/SKILL.md"
  echo "the user's config" > "$outside/$var/config.yaml"
  export "$var=$outside/$var"
done
# A vault under Git, with a .stignore that setup would amend.
mkdir -p "$outside/vault"
echo "the user's note" > "$outside/vault/note.md"
echo "the user's ignores" > "$outside/vault/.stignore"
git -C "$outside/vault" init -q
git -C "$outside/vault" add -A
git -C "$outside/vault" -c user.name=sentinel -c user.email=sentinel@example.invalid commit -q -m "the user's vault"
export PETIT_POUCET_VAULT="$outside/vault"
snapshot() {
  (cd "$outside" && find . | sort | while read -r path; do
    [ -d "$path" ] && echo "$path/" || echo "$path: $(cksum < "$path")"
  done
  git -C "$outside/vault" status --porcelain
  git -C "$outside/vault" rev-parse HEAD)
}
before=$(snapshot)
sh "$(dirname "$0")/$1.sh"
[ "$(snapshot)" = "$before" ] || { echo "FAIL: $1.sh changed a profile or vault outside its HOME:" >&2; snapshot >&2; exit 1; }

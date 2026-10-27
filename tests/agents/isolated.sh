#!/bin/sh
# Runs one agent's script with every agent folder variable naming a profile of the user's, then checks the
# script left those profiles as they were: the tests stay in their throwaway HOME.
# Usage: sh tests/agents/isolated.sh <agent>
set -eu
outside=$(mktemp -d)
for var in COPILOT_HOME CODEX_HOME HERMES_HOME; do
  mkdir -p "$outside/$var/skills/memory"
  echo "the user's skill" > "$outside/$var/skills/memory/SKILL.md"
  echo "the user's config" > "$outside/$var/config.yaml"
  export "$var=$outside/$var"
done
snapshot() {
  (cd "$outside" && find . | sort | while read -r path; do
    [ -d "$path" ] && echo "$path/" || echo "$path: $(cat "$path")"
  done)
}
before=$(snapshot)
sh "$(dirname "$0")/$1.sh"
[ "$(snapshot)" = "$before" ] || { echo "FAIL: $1.sh changed a profile outside its HOME:" >&2; snapshot >&2; exit 1; }

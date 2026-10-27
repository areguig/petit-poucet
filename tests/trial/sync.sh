#!/bin/sh
# Three Linux machines in containers, each with its own distro's Syncthing (v1 from apt and dnf, v2 from the
# official release) and real Claude Code on Haiku, share one memory through `petit-poucet sync`.
# Usage: sh tests/trial/sync.sh <petit-poucet binary for linux x86_64>
# Needs Docker and a Claude Code token (`claude setup-token`) in ~/.config/pp-trial/claude-token or $TOKEN_FILE.
# KEEP=1 leaves the containers running afterwards.
set -eu
bin=$(cd "$(dirname "$1")" && pwd)/$(basename "$1")
here=$(cd "$(dirname "$0")" && pwd)
token_file=${TOKEN_FILE:-$HOME/.config/pp-trial/claude-token}
machines="debian fedora ubuntu"
net=pp-trial
failures=0

check() { # check <what> <command...>
  what=$1; shift
  if "$@"; then echo "PASS $what"; else echo "FAIL $what"; failures=$((failures + 1)); fi
}
on() { docker exec -u dev -w /home/dev/work "pp-$1" bash -lc "$2"; }
claude_on() {
  docker exec -u dev -w /home/dev/work -e PROMPT="$2" "pp-$1" \
    bash -lc 'claude -p --model haiku --allowedTools mcp__petit-poucet -- "$PROMPT"'
}
contains() { printf %s "$1" | grep -qi -- "$2"; }
fails_with() { # fails_with <machine> <command> <text>: the command fails and prints the text
  out=$(on "$1" "$2") && return 1
  contains "$out" "$3"
}
wait_for() { # wait_for <what> <seconds> <command...>
  what=$1; limit=$2; shift 2; waited=0
  until "$@" 2>/dev/null; do
    sleep 2; waited=$((waited + 2))
    if [ "$waited" -ge "$limit" ]; then echo "FAIL waited ${limit}s for $what"; failures=$((failures + 1)); return 1; fi
  done
  echo "PASS $what (${waited}s)"
}
has_file() { on "$1" "test -f ~/agent-memory/$2"; }
conflict_on() { on "$1" "ls ~/agent-memory/Preferences | grep -q sync-conflict"; }
no_conflict_on() { ! conflict_on "$1"; }
cleanup() {
  [ "${KEEP:-}" = 1 ] && return
  for m in $machines; do docker rm -f "pp-$m" >/dev/null 2>&1 || true; done
  docker network rm "$net" >/dev/null 2>&1 || true
}
trap cleanup EXIT

echo "== images"
docker build -q --platform linux/amd64 -t pp-trial-debian --build-arg BASE=debian:13 "$here" >/dev/null
docker build -q --platform linux/amd64 -t pp-trial-fedora --build-arg BASE=fedora:42 "$here" >/dev/null
docker build -q --platform linux/amd64 -t pp-trial-ubuntu --build-arg BASE=ubuntu:24.04 \
  --build-arg SYNCTHING=v2.1.6 \
  --build-arg SYNCTHING_SHA256=524ef4e1df1850b719e2378c8369956f4e5f2ab9a46fdf7f45f2b925012147aa "$here" >/dev/null

echo "== machines"
cleanup
docker network create "$net" >/dev/null
CLAUDE_CODE_OAUTH_TOKEN=$(cat "$token_file")
export CLAUDE_CODE_OAUTH_TOKEN
for m in $machines; do
  docker run -d --name "pp-$m" --hostname "$m" --network "$net" --platform linux/amd64 \
    -e CLAUDE_CODE_OAUTH_TOKEN -v "$bin:/usr/local/bin/petit-poucet:ro" "pp-trial-$m" sleep infinity >/dev/null
  echo "$m: $(on "$m" 'syncthing --version | cut -d" " -f1-2'), $(on "$m" 'claude --version')"
done

echo "== setup, then Syncthing, on each machine"
setup=$(on debian "petit-poucet setup")
check "setup without a git identity says how to set one" contains "$setup" "vault: set your git identity"
check "debian: sync before Syncthing runs says how to start it" \
  fails_with debian "petit-poucet sync" "Syncthing is installed but doesn't answer"
for m in $machines; do
  on "$m" "petit-poucet setup >/dev/null; git config --global user.name dev; git config --global user.email dev@$m"
  on "$m" "setsid -f syncthing serve --no-browser --no-restart > ~/syncthing.log 2>&1"
done
for m in $machines; do wait_for "$m: Syncthing answers" 60 on "$m" "syncthing cli show system >/dev/null"; done

echo "== pairing: debian is the hub, as a home server would be"
id_of() { on "$1" "petit-poucet sync" | sed -n 's/^on another machine, run: petit-poucet sync //p'; }
debian=$(id_of debian); fedora=$(id_of fedora); ubuntu=$(id_of ubuntu)
on fedora "petit-poucet sync $debian" >/dev/null
on ubuntu "petit-poucet sync $(printf %s "$debian" | tr 'A-Z' 'a-z')" >/dev/null
on debian "petit-poucet sync $fedora" >/dev/null
on debian "petit-poucet sync $ubuntu" >/dev/null
again=$(on debian "petit-poucet sync $fedora")
check "running sync again changes nothing" sh -c "! printf %s \"\$1\" | grep -q 'paired with\|vault shared'" _ "$again"
wait_for "debian connected to fedora and ubuntu" 120 sh -c \
  "docker exec -u dev pp-debian petit-poucet sync | grep -q 'syncing with: .*(connected).*(connected)'"

echo "== a note saved on fedora reaches every machine, and Claude Code loads it there"
claude_on fedora "Remember this as a user preference for all repos: Dockerfiles pin base images by digest. Save it to memory now." >/dev/null
note=$(on fedora "grep -l 'by digest' ~/agent-memory/Preferences/*.md | head -1 | xargs basename")
check "fedora saved it as Preferences/$note" test -n "$note"
wait_for "the note on debian" 120 has_file debian "Preferences/$note"
wait_for "the note on ubuntu, through debian" 120 has_file ubuntu "Preferences/$note"
answer=$(claude_on ubuntu "Without using any tool, answer only YES or NO: does your loaded memory say how Dockerfiles should pin base images?")
check "Claude Code on ubuntu knows it at session start (answered: $answer)" contains "$answer" yes
check "ubuntu committed what arrived" sh -c "docker exec -u dev pp-ubuntu git -C /home/dev/agent-memory log --format=%s | grep -q 'sync: notes changed outside petit-poucet'"
check "fedora's commit names Claude Code" sh -c "docker exec -u dev pp-fedora git -C /home/dev/agent-memory log --format=%s | grep -q '(claude-code)'"
check "git history stays on each machine" sh -c "! docker exec -u dev pp-ubuntu git -C /home/dev/agent-memory log --format=%s | grep -q '^create: Preferences/$(basename "$note" .md)'"

echo "== the same note changed on fedora (offline) and on debian: a conflict, merged by the agent"
docker network disconnect "$net" pp-fedora
claude_on fedora "Update my memory note about Dockerfile base images: also say that base images are rebuilt weekly. Read it first, then update it." >/dev/null
sleep 2
claude_on debian "Update my memory note about Dockerfile base images: also say that only official images are allowed. Read it first, then update it." >/dev/null
docker network connect "$net" pp-fedora
wait_for "a conflict copy on debian" 240 conflict_on debian
hook=$(on debian "echo '{}' | petit-poucet hook session-start --agent claude")
check "the user is told to ask the agent" contains "$hook" "note has a sync conflict: ask your agent to merge it"
check "the agent is told which copy is older" contains "$hook" "older copy of Preferences/$note"
claude_on debian "Merge the sync conflict you were told about. My decision: keep both additions, weekly rebuilds and official images only. Then delete the copy." >/dev/null
for m in $machines; do
  wait_for "no conflict copy left on $m" 120 no_conflict_on "$m"
done
for m in $machines; do
  wait_for "$m: the merged note has both additions" 120 sh -c \
    "docker exec -u dev pp-$m grep -qi weekly /home/dev/agent-memory/Preferences/$note && docker exec -u dev pp-$m grep -qi official /home/dev/agent-memory/Preferences/$note"
done
for m in $machines; do
  on "$m" "echo '{}' | petit-poucet hook session-start --agent claude >/dev/null"
  check "$m: check passes" on "$m" "petit-poucet check >/dev/null"
  check "$m: nothing left uncommitted but usage" sh -c "test -z \"\$(docker exec -u dev pp-$m git -C /home/dev/agent-memory status --porcelain | grep -v .usage/)\""
done

echo "== $failures failed"
[ "$failures" = 0 ]

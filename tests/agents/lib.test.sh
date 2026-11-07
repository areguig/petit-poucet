#!/bin/sh
# Fixture test of start_model (#174): a `node` on PATH stands in for the mock model, in a repo whose binary is a stub.
# Usage: sh tests/agents/lib.test.sh
set -eu
lib="$(cd "$(dirname "$0")" && pwd)/lib.sh"
snapshot_helper="$(cd "$(dirname "$0")" && pwd)/isolation_snapshot.sh"
work=$(mktemp -d)
mkdir -p "$work/repo/target/release" "$work/bin"
printf '#!/bin/sh\n' > "$work/repo/target/release/petit-poucet"
cp "$work/repo/target/release/petit-poucet" "$work/repo/target/release/petit-poucet.exe"
chmod +x "$work/repo/target/release/petit-poucet" "$work/repo/target/release/petit-poucet.exe"
PATH="$work/bin:$PATH"
fail() { echo "FAIL: $*" >&2; exit 1; }
. "$snapshot_helper"

outside="$work/outside"
mkdir -p "$outside/vault"
printf 'the user note\n' > "$outside/vault/note.md"
git -C "$outside/vault" init -q
git -C "$outside/vault" add -A
git -C "$outside/vault" -c user.name=sentinel -c user.email=sentinel@example.invalid commit -q -m initial
before=$(isolation_snapshot "$outside")
mkdir -p "$outside/vault/.git/objects"
printf 'maintenance\n' > "$outside/vault/.git/objects/maintenance.lock"
[ "$(isolation_snapshot "$outside")" = "$before" ] || fail "snapshot included transient Git metadata"
rm "$outside/vault/.git/objects/maintenance.lock"
printf 'changed\n' >> "$outside/vault/note.md"
[ "$(isolation_snapshot "$outside")" != "$before" ] || fail "snapshot missed an outside vault change"

# fixture <body>: the `node` start_model runs; FIXTURE_PID receives its pid.
fixture() {
  printf '#!/bin/sh\necho $$ > "%s"\n%s\n' "$work/fixture-pid" "$1" > "$work/bin/node"
  chmod +x "$work/bin/node"
  rm -f "$work/fixture-pid"
}

# start <limit>: runs start_model then a request to the model, in a subshell killed after <limit> seconds if still running.
# Sets status, out (stdout and stderr) and elapsed.
start() {
  begin=$(date +%s)
  (cd "$work/repo" && . "$lib" && start_model && curl -fsS "$model_url/") > "$work/out" 2>&1 &
  pid=$!
  tries=0
  while kill -0 "$pid" 2>/dev/null && [ "$tries" -lt $(($1 * 10)) ]; do
    tries=$((tries + 1))
    sleep 0.1
  done
  if kill "$pid" 2>/dev/null; then
    kill "$(cat "$work/fixture-pid")" 2>/dev/null || true
    fail "start_model still waiting after $1 s"
  fi
  wait "$pid" && status=0 || status=$?
  elapsed=$(($(date +%s) - begin))
  out=$(cat "$work/out")
}

# gone: the fixture's process didn't outlive start_model's shell.
gone() {
  tries=0
  while kill -0 "$(cat "$work/fixture-pid")" 2>/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -le 20 ] || fail "the model fixture outlived start_model"
    sleep 0.1
  done
}

# A process of the caller's, which start_model must leave running.
sleep 300 &
bystander=$!
trap 'kill $bystander 2>/dev/null' EXIT

fixture 'exit 23'
start 5
[ "$status" -ne 0 ] || fail "start_model succeeded with a model that exited"
printf '%s\n' "$out" | grep -q 'mock model exited (23) before printing its port' || fail "no exit reason in: $out"
[ "$elapsed" -le 2 ] || fail "start_model took $elapsed s to notice the model exited"

fixture 'exec sleep 300'
start 30
[ "$status" -ne 0 ] || fail "start_model succeeded with a model that printed no port"
printf '%s\n' "$out" | grep -q 'mock model printed no port in 20 s' || fail "no deadline reason in: $out"
[ "$elapsed" -le 25 ] || fail "start_model took $elapsed s past its deadline"
gone

# A loaded host, like the macOS runner: each `sleep 0.1` takes 0.15 s; the 20 s deadline is still 20 s of real time.
mkdir -p "$work/slow"
printf '#!/bin/sh\n%s 0.05\nexec %s "$@"\n' "$(command -v sleep)" "$(command -v sleep)" > "$work/slow/sleep"
chmod +x "$work/slow/sleep"
path=$PATH
PATH="$work/slow:$PATH"
start 40
PATH=$path
[ "$status" -ne 0 ] || fail "start_model succeeded with a model that printed no port on a slow host"
printf '%s\n' "$out" | grep -q 'mock model printed no port in 20 s' || fail "no deadline reason on a slow host in: $out"
[ "$elapsed" -le 25 ] || fail "start_model took $elapsed s past its deadline on a slow host"
gone

# A model that prints its port and answers: perl, which Git for Windows ships as well.
fixture "exec perl -MIO::Socket::INET -e '
  \$server = IO::Socket::INET->new(LocalAddr => \"127.0.0.1\", LocalPort => 0, Listen => 5, ReuseAddr => 1) or die;
  \$| = 1;
  print \$server->sockport;
  while (\$client = \$server->accept) { <\$client>; print \$client \"HTTP/1.0 200 OK\\r\\n\\r\\nok\"; close \$client }'"
start 10
[ "$status" -eq 0 ] || fail "start_model failed with a healthy model: $out"
[ "$out" = ok ] || fail "the healthy model answered: $out"
gone

kill -0 "$bystander" 2>/dev/null || fail "start_model killed a process it didn't start"
echo "start_model: ok"

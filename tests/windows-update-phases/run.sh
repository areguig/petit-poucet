#!/usr/bin/env bash
# Diagnostic for #165, Windows CI only: the whole update suite, against a copy of self-replace 1.5.0 whose Windows
# steps log their outcome to stderr, repeated until it fails or 5 minutes pass. Never used for a release build.
# Usage: bash tests/windows-update-phases/run.sh <new folder outside the repository>
set -euo pipefail
fail() { echo "FAIL: $*" >&2; exit 1; }
# Git Bash on Windows: bash takes /c/... paths, cargo C:/... ones.
unix() { if command -v cygpath >/dev/null; then cygpath -u "$1"; else echo "$1"; fi; }
mixed() { if command -v cygpath >/dev/null; then cygpath -m "$1"; else echo "$1"; fi; }
here=$(cd "$(dirname "$0")" && pwd)
cd "$here/../.."
work=$(unix "$1")
crate=$work/self-replace-1.5.0
upstream=a633cf48c01b453137cb3195cd8371fbb65e0117e5426df64744553c9021093f
instrumented=aa5742f195312c3114455ea34335050e5c83d7d6347a536932cc4ec47b11a778
digest() { sha256sum < "$1" | cut -d' ' -f1; }

sources=("$(unix "${CARGO_HOME:-$HOME/.cargo}")"/registry/src/*/self-replace-1.5.0)
[ ${#sources[@]} = 1 ] && [ -d "${sources[0]}" ] || fail "not one cached self-replace 1.5.0: ${sources[*]}"
mkdir "$work" || fail "$work: give a new folder"
! git -C "$work" rev-parse 2>/dev/null || fail "$work is inside a repository: git apply would patch it"
cp -R "${sources[0]}" "$crate"
[ "$(digest "$crate/src/windows.rs")" = "$upstream" ] || fail "self-replace 1.5.0 differs from the instrumented one"
# Git for Windows defaults to core.autocrlf=true, which would write the patched file with CRLF endings.
git -c core.autocrlf=false -C "$crate" apply "$here/self-replace.patch"
[ "$(digest "$crate/src/windows.rs")" = "$instrumented" ] || fail "the instrumentation did not apply as written"

# The path override drops the registry source from Cargo.lock: restored on exit, and nothing else may change.
cp Cargo.lock "$work/Cargo.lock"
trap 'cp "$work/Cargo.lock" Cargo.lock' EXIT
export CARGO_TARGET_DIR=$work/target
cargo=(cargo --offline --config "patch.crates-io.self-replace.path='$(mixed "$crate")'")
"${cargo[@]}" test --test update --no-run
expected='< source = "registry+https://github.com/rust-lang/crates.io-index"
< checksum = "03ec815b5eab420ab893f63393878d89c90fdd94c0bcc44c07abb8ad95552fb7"'
[ "$(diff "$work/Cargo.lock" Cargo.lock | grep '^[<>]')" = "$expected" ] || fail "Cargo.lock changed beyond self-replace's source"
bin=$CARGO_TARGET_DIR/debug/petit-poucet
[ -f "$bin.exe" ] && bin=$bin.exe
grep -qa 'self-replace phase' "$bin" || fail "$bin is not instrumented"

# Positive control: a sharing violation injected at the first rename shows its phase and holders.
control=$("${cargo[@]}" test --test update update_leaves_a_binary_another_program_holds_whole -- --exact --nocapture 2>&1) \
  || { echo "$control"; fail "the injected sharing violation test failed"; }
echo "$control"
grep -q 'self-replace phase rename installed to relocated: os error 32' <<<"$control" || fail "no phase for the injected violation"
# The test binary (update-<hash>.exe) holds it.
grep -Eq 'self-replace holders of petit-poucet\.exe \(self [0-9]+\): .*[0-9]+:update-[0-9a-f]+\.exe' <<<"$control" \
  || fail "the injected violation's holder is not named"

# The original failure came from the whole update suite at its default test concurrency, so repeat exactly that.
echo "runner CPUs: $(nproc), RUST_TEST_THREADS=${RUST_TEST_THREADS-unset}"
# A 5 s timing test unrelated to self-replace can also fail under parallel load. Such a run still counts, and the
# script still exits nonzero, but the window goes on to observe the updater. Any other failure stops at once.
timer=session_start_never_waits_for_the_check
only_timer_failed() {
  [ "$(grep -E '^test .* \.\.\. FAILED$' "$1")" = "test $timer ... FAILED" ] \
    && [ "$(grep -Ec '^test result: ' "$1")" = 1 ] \
    && grep -Eq '^test result: FAILED\. [0-9]+ passed; 1 failed;' "$1"
}
log=$work/suite.log
runs=0 timer_failures=0 first_status=0
deadline=$((SECONDS + 300))
while [ $SECONDS -lt $deadline ]; do
  runs=$((runs + 1))
  echo "full update suite, run $runs"
  # The suite's injected-violation test passes when its update fails: only cargo's exit status counts.
  set +e
  "${cargo[@]}" test --test update | tee "$log"
  statuses=("${PIPESTATUS[@]}")
  set -e
  [ "${statuses[1]}" = 0 ] || fail "could not record full update suite run $runs"
  status=${statuses[0]}
  [ $status = 0 ] && continue
  if [ $status = 101 ] && only_timer_failed "$log"; then
    timer_failures=$((timer_failures + 1))
    [ $first_status != 0 ] || first_status=$status
    echo "UNRELATED FAIL: full update suite run $runs, only $timer failed, cargo exit $status; observing on" >&2
    continue
  fi
  echo "FAIL: full update suite run $runs, cargo exit $status" >&2
  exit $status
done
echo "$runs full update suite runs: no updater failure; $timer failed alone in $timer_failures"
[ $first_status = 0 ] || { echo "FAIL: $timer failed in $timer_failures of $runs runs, first cargo exit $first_status" >&2; exit $first_status; }

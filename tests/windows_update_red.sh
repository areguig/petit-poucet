#!/usr/bin/env bash
# CI only, on Windows: with the fix undone (update calls self_replace once), the caller test must fail on os error 32.
# The source is restored byte for byte on every exit, so the full suite that follows runs on the fixed code.
# Usage, from the checkout's root: bash tests/windows_update_red.sh
set -euo pipefail
test_name=update_waits_for_a_program_that_briefly_holds_the_binary
source=src/upgrade.rs
fixed='replace::replace(download.path(), exe)'
old='self_replace::self_replace(download.path())'
fail() { echo "FAIL: $*" >&2; exit 1; }

[ "${GITHUB_ACTIONS:-}" = true ] || fail "runs only in CI: it edits $source in the checkout"
prefix=$(git rev-parse --show-prefix)
[ -z "$prefix" ] || fail "run from the checkout's root"
git diff --quiet HEAD -- "$source" || fail "$source differs from HEAD before the proof"
[ "$(grep -cF "$fixed" "$source")" = 1 ] || fail "$source must call '$fixed' exactly once"

scratch=$(mktemp -d "${RUNNER_TEMP:?}/update-red.XXXXXX")
cp "$source" "$scratch/upgrade.rs" && cmp -s "$source" "$scratch/upgrade.rs" \
  || { rm -rf "$scratch"; fail "could not back up $source"; }
restore() {
  local code=$?
  cp "$scratch/upgrade.rs" "$source" || { echo "FAIL: could not copy the backup over $source" >&2; exit 1; }
  cmp -s "$scratch/upgrade.rs" "$source" || { echo "FAIL: $source differs from its backup" >&2; exit 1; }
  git diff --quiet HEAD -- "$source" || { echo "FAIL: $source differs from HEAD after restore" >&2; exit 1; }
  rm -rf "$scratch"
  exit "$code"
}
trap restore EXIT
trap 'exit 130' INT TERM

sed "s/replace::replace(download\.path(), exe)/$old/" "$scratch/upgrade.rs" > "$source"
[ "$(diff "$scratch/upgrade.rs" "$source" | grep -c '^[<>]')" = 2 ] && [ "$(grep -cF "$old" "$source")" = 1 ] \
  || fail "undoing the fix must change exactly the one call"

cargo test --test update "$test_name" -- --exact > "$scratch/raw.log" 2>&1 && status=0 || status=$?
sed 's/\x1b\[[0-9;]*[A-Za-z]//g; s/\r$//' "$scratch/raw.log" > "$scratch/test.log"
cat "$scratch/test.log"

[ "$status" = 101 ] || fail "expected exit 101, got $status"
grep -qxF 'running 1 test' "$scratch/test.log" || fail "expected exactly one selected test"
grep -qxF "test $test_name ... FAILED" "$scratch/test.log" || fail "expected $test_name to fail"
grep -q '^test result: FAILED\. 0 passed; 1 failed;' "$scratch/test.log" || fail "expected one failed test"
# The update's own error, then the blank line and marker that only the update.success assertion prints.
awk 'two_back ~ /^petit-poucet: replacing .*\(os error 32\)$/ && one_back == "" && /^next to the binary: \[/ { found = 1 }
  { two_back = one_back; one_back = $0 } END { exit !found }' "$scratch/test.log" \
  || fail "expected the update.success assertion to report the update failing on os error 32"
echo "RED as expected: without the fix, $test_name fails on os error 32"

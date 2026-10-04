# Shared by the agent scripts: run from the repo root after `cargo build --release`.
set -eu

pp="$PWD/target/release/petit-poucet"
[ -x "$pp" ] || [ -x "$pp.exe" ] || { echo "build first: cargo build --release" >&2; exit 1; }
repo="$PWD"
# A throwaway HOME, so a local run never touches the real agents' config or vault.
HOME=$(mktemp -d)
export HOME
# On Windows (Git Bash), programs find the home folder through USERPROFILE.
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    windows=1
    USERPROFILE=$(cygpath -w "$HOME")
    # Codex finds the profile folder through Windows itself, not USERPROFILE.
    CODEX_HOME="$USERPROFILE\.codex"
    export USERPROFILE CODEX_HOME
    # A folder with a space, like many Windows profiles: the hooks must still find the binary.
    mkdir -p "$HOME/petit poucet"
    cp "$pp.exe" "$HOME/petit poucet/petit-poucet.exe"
    pp="$HOME/petit poucet/petit-poucet"
    ;;
  *) windows= ;;
esac

# The latest release, as GitHub's API describes it, served from a local file to the sessions' update check.
echo '{"tag_name": "v99.0.0"}' > "$HOME/latest.json"
release_path="$HOME/latest.json"
[ -z "$windows" ] || release_path="/$(cygpath -m "$release_path")"
export PETIT_POUCET_LATEST_RELEASE="file://$release_path"

fail() { echo "FAIL: $*" >&2; exit 1; }
# has <text> <pattern>: one line of text matches the grep pattern.
has() { printf '%s\n' "$1" | grep -q -- "$2" || fail "expected /$2/ in:
$1"; }

# start_model: runs the mock model (model.mjs, after `npm ci --prefix tests/agents`); agents reach it at $model_url.
start_model() {
  calls="$HOME/calls.jsonl"
  node "$repo/tests/agents/model.mjs" "$calls" > "$HOME/model-port" &
  model_pid=$!
  trap 'kill $model_pid' EXIT
  while [ ! -s "$HOME/model-port" ]; do sleep 0.1; done
  model_url="http://127.0.0.1:$(cat "$HOME/model-port")"
  # A proxy set for the machine must not stand between the agent and the mock.
  export NO_PROXY=127.0.0.1 no_proxy=127.0.0.1
}

# check_calls <tools marker>: what the session sent the model; see calls.mjs.
check_calls() {
  node "$repo/tests/agents/calls.mjs" "$calls" "$1" || fail "petit-poucet didn't reach the model as expected"
}

# check_update: the update check that session start leaves running in the background outlived the agent's hook.
check_update() {
  latest="$HOME/agent-memory/.petit-poucet/latest-release"
  tries=0
  until [ "$(cat "$latest" 2>/dev/null)" = 99.0.0 ]; do
    tries=$((tries + 1))
    [ "$tries" -le 200 ] || fail "the update check never recorded the latest release in $latest"
    sleep 0.1
  done
}

# install_old_plugin <claude|copilot>: installs the petit-poucet 0.2.2 plugin, the version `setup` replaces.
# It only ever ran on macOS and Linux (its launcher was a shell script), so on Windows there is nothing to install.
install_old_plugin() {
  [ -z "$windows" ] || return 0
  git clone -q --depth 1 --branch v0.2.2 https://github.com/areguig/petit-poucet "$HOME/old-plugin"
  "$1" plugin marketplace add "$HOME/old-plugin" >/dev/null
  "$1" plugin install petit-poucet@petit-poucet >/dev/null
}

# big_vault: a vault too large for one memory_review result (#77); the mock calls $REVIEW_TOOL on the prompt petit-poucet-review.
big_vault() {
  big="$HOME/agent-memory/Projects/big"
  mkdir -p "$big"
  i=0
  while [ "$i" -lt 250 ]; do
    printf -- '---\ntype: project\nscope: big\nsummary: fact %s of a large vault, worded like a real one-line summary\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n# Fact %s\n' "$i" "$i" > "$big/fact-$i.md"
    i=$((i + 1))
  done
  : > "$calls"
}

# check_review: the review reached the model whole, not saved to a file or cut in the middle.
check_review() {
  grep -q 'page 1 of ' "$calls" || fail "memory_review didn't reach the model inline"
}

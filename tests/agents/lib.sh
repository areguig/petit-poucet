# Shared by the agent scripts: run from the repo root after `cargo build --release`.
set -eu

pp="$PWD/target/release/petit-poucet"
[ -x "$pp" ] || [ -x "$pp.exe" ] || { echo "build first: cargo build --release" >&2; exit 1; }
repo="$PWD"
# A throwaway HOME, so a local run never touches the real agents' config or vault. The agents' own folder
# variables would lead outside it: each points inside, Hermes's too, whose Windows default is under LOCALAPPDATA.
HOME=$(mktemp -d)
COPILOT_HOME="$HOME/.copilot"
CODEX_HOME="$HOME/.codex"
HERMES_HOME="$HOME/.hermes"
export HOME COPILOT_HOME CODEX_HOME HERMES_HOME
# An inherited vault would lead outside HOME too: the default vault is $HOME/agent-memory.
unset PETIT_POUCET_VAULT
# On Windows (Git Bash), programs find the home folder through USERPROFILE, and take Windows paths.
case "$(uname -s)" in
  MINGW* | MSYS* | CYGWIN*)
    windows=1
    USERPROFILE=$(cygpath -w "$HOME")
    COPILOT_HOME="$USERPROFILE\.copilot"
    # Codex finds the profile folder through Windows itself, not USERPROFILE.
    CODEX_HOME="$USERPROFILE\.codex"
    HERMES_HOME="$USERPROFILE\.hermes"
    export USERPROFILE
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
  trap 'kill $model_pid 2>/dev/null' EXIT
  # Wall-clock time, not a count of sleeps: on a loaded host each `sleep 0.1` takes longer.
  deadline=$(($(date +%s) + 20))
  until [ -s "$HOME/model-port" ]; do
    if ! kill -0 "$model_pid" 2>/dev/null; then
      wait "$model_pid" && status=0 || status=$?
      fail "the mock model exited ($status) before printing its port"
    fi
    [ "$(date +%s)" -lt "$deadline" ] || fail "the mock model printed no port in 20 s"
    sleep 0.1
  done
  model_url="http://127.0.0.1:$(cat "$HOME/model-port")"
  # A proxy set for the machine must not stand between the agent and the mock.
  export NO_PROXY=127.0.0.1 no_proxy=127.0.0.1
}

# session_view_of <agent> <file>: what that agent's hook replies at session start, from a folder without a project.
session_view_of() {
  (cd "$(mktemp -d)" && echo '{}' | "$pp" hook session-start --agent "$1") > "$2"
}

# oversize_memory <agent>: more preferences than a session loads within its budget; check_calls then expects
# the whole session view that agent's hook produces for them, and removes them.
# The view before the update check has found a release is taken now, with the check off so it starts nothing:
# the session's own hook starts the check, and check_update still waits for it.
oversize_memory() {
  session_agent=$1
  i=0
  while [ "$i" -lt 150 ]; do
    printf -- '---\ntype: user\nscope: all repos\nsummary: preference %s, worded like a real one-line summary of a rule the user stated about how they work\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n**Why:** x\n' "$i" > "$HOME/agent-memory/Preferences/oversize-$i.md"
    i=$((i + 1))
  done
  PETIT_POUCET_NO_UPDATE_CHECK=1 session_view_of "$1" "$HOME/session-start-before.json"
}

# check_calls <tools marker> [calls.mjs options]: what the session sent the model; see calls.mjs.
check_calls() {
  if [ -n "${session_agent:-}" ]; then
    # A call carries the view from before the update check or, once it has recorded its release, the one with its notice.
    check_update
    session_view_of "$session_agent" "$HOME/session-start-after.json"
    budget=$(sed -n 's/^session_start_max_bytes = //p' "$HOME/.config/petit-poucet/config.toml")
    set -- "$@" "--session=$HOME/session-start-before.json" "--session=$HOME/session-start-after.json" "--budget=$budget"
  fi
  node "$repo/tests/agents/calls.mjs" "$calls" "$@" || fail "petit-poucet didn't reach the model as expected"
  rm -f "$HOME"/agent-memory/Preferences/oversize-*.md
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

# big_vault: a vault too large for one memory_review page (#77); the mock calls $REVIEW_TOOL, for page 1, on the prompt petit-poucet-review.
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

# check_review: the first review page reached the model whole, in a tool result, not saved to a file or cut; see review.mjs.
check_review() {
  node "$repo/tests/agents/review.mjs" "$calls" || fail "the first memory_review page didn't reach the model whole and inline"
}

# probe_note <agent>: a note whose long slug the session view shortens; the mock model reads it by the name the view shows
# when a prompt says petit-poucet-read (model.mjs). check_read then expects the note's body in a tool result.
probe_body="probe-body-3f9a1c"
probe_note() {
  printf -- '---\ntype: user\nscope: all repos\nsummary: short-name probe\ncreated: 2026-09-01\ntags: [agent-memory]\n---\n# Probe\n\n%s\n' "$probe_body" > "$HOME/agent-memory/Preferences/always-probe-the-short-name-before-reading.md"
  PETIT_POUCET_NO_UPDATE_CHECK=1 session_view_of "$1" "$HOME/probe-view.json"
  grep -q -F '[[always-probe-the-short]] — short-name probe' "$HOME/probe-view.json" || fail "the session view doesn't show the probe note by its short name"
  : > "$calls"
}

check_read() {
  node "$repo/tests/agents/holds.mjs" "$calls" "$probe_body" || fail "the note read by its short name didn't reach the model"
}

# The mock model creates a preference when a prompt says petit-poucet-save (model.mjs); check_save then expects the scope
# reminder (#215) in a tool result. preference_json is the call's arguments.
scope_reminder="Preferences load in every session of every repo: make sure this is cross-project and says when it applies, and confirm with the user when unsure; otherwise move it to the project or a topic (memory_move)."
preference_json='{"type": "feedback", "scope": "all repos", "title": "Scope probe", "summary": "scope probe", "fact": "A fact.", "source": "the agent test", "how_to_apply": "Never."}'
before_save() { : > "$calls"; }

check_save() {
  node "$repo/tests/agents/holds.mjs" "$calls" "$scope_reminder" || fail "the scope reminder of a new preference didn't reach the model"
}

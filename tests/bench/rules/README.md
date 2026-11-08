# Session-start rules bench

Runs the same scripted sessions against two petit-poucet binaries (baseline and candidate) on real cheap models,
and scores what each session left in the vault. By hand only, not in CI: every run costs real model calls.

    cargo build --release
    python3 -I tests/bench/rules/bench.py --baseline <bin> --candidate <bin> [--agents claude,copilot] [--runs 3] [--cases decision,topic]
    python3 -I tests/bench/rules/bench.py --self-test    # scorers against hand-made vaults, no model

Models are fixed: Claude Code `haiku`, Copilot CLI `gpt-6-luna`. Each run gets a fresh temp vault, a temp git project
(`bench-app`, with a README) and a temp config: Claude Code with `--setting-sources '' --settings`, `--strict-mcp-config`;
Copilot with a temp `HOME`/`COPILOT_HOME` (the token comes from `gh auth token`). Nothing touches `~/agent-memory`
or the real agent configs. The agent is wired as `setup` does: the binary's `serve`, and its SessionStart and Stop hooks.
A session ends after one prompt, so the Stop hook replays the binary's real reminder (the one it gives at the third
stop) once.

| case | the user... | passes when |
|---|---|---|
| decision | states a project decision | exactly one note saved, holding it |
| topic | states a fact about their homelab | a `Topics/` note holds it |
| repo-fact | asks what the README says | no note written |
| update | corrects a seeded project note | the note holds the correction, no new note, and it was read before it was saved |
| feedback | muses against a seeded feedback note without deciding | the feedback note is unchanged |
| secret | gives a token and asks to remember the endpoint | no note holds the token |

Results go to a temp folder (printed first): `results.jsonl`, one line per run, and each run's raw agent output.
The summary gives per case, agent and variant the pass rate, mean notes saved, duplicate notes (a new note whose title
and summary overlap half or more with an existing one), session-start bytes, and runs with a harness problem (agent
exit code not 0, or the Stop hook never fired), which never count as a pass. With a few runs, read the rates as hints, not proof.

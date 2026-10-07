<p align="center"><img src="site/logo.svg" width="120" alt=""></p>

<h1 align="center">petit-poucet</h1>

<p align="center"><em>Leaves pebbles so your coding agents find their way back.</em></p>

<p align="center">
  <a href="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml"><img src="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/areguig/petit-poucet/releases/latest"><img src="https://img.shields.io/github/v/release/areguig/petit-poucet" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/areguig/petit-poucet" alt="Apache-2.0"></a>
</p>

In the French tale *Le Petit Poucet*, a boy drops white pebbles along the path so he can find his way home. **petit-poucet** does the same for AI coding agents: it keeps a small, curated memory of your rules, decisions and verified facts, and every agent you switch between starts each session with it.

Works with **Claude Code, GitHub Copilot (CLI, VS Code, IntelliJ, the Copilot app), Codex, Cursor, Antigravity CLI and OpenCode**, on macOS, Linux and Windows.

## Why

Coding agents forget everything between sessions, and each keeps its own memory in its own format: what you taught Claude Code on Monday, Codex doesn't know on Tuesday. petit-poucet gives them one memory that:

- **follows you across agents**: one command wires the same memory into every agent it finds;
- **is there from the first message**: a session-start hook hands the agent your rules, so nothing depends on the model deciding to look;
- **is yours to read**: plain Markdown in a folder you choose, browsable in [Obsidian](https://obsidian.md), with every change in git;
- **is curated**: agents save a decision or a verified fact on purpose, with its source, and rules you stated change only with your confirmation;
- **stays clean**: checks and a cleanup review catch duplicates, stale facts and unused notes.

How it compares with the agents' own memory: [comparison](docs/comparison.md).

## Install

```sh
# macOS, Linux
curl -fsSL https://raw.githubusercontent.com/areguig/petit-poucet/main/install.sh | sh
petit-poucet setup
```

```powershell
# Windows, in PowerShell
irm https://raw.githubusercontent.com/areguig/petit-poucet/main/install.ps1 | iex
petit-poucet setup
```

Then restart your agent. Already keeping memory elsewhere? Ask your agent to run the `migrate-memory` skill. Details, per-agent notes and settings: [install and set up](docs/setup.md).

## What your agent sees

At the start of every session, the agent gets your rules and an Index of what applies here: your preferences and the current repo's notes, one line each. It opens only the notes a task needs.

```
🪨 petit-poucet · 20 notes loaded (preferences + chargepath-api)
```

```markdown
## Preferences (all repos)
- [[Preferences/commit-rules]] — commit locally per step, 1-2 line message, never push
- [[Preferences/never-read-secrets]] — use secret values only by piping them into commands, never print them

## Projects / chargepath-api
- [[Projects/chargepath-api/prod-db-no-postgis]] — the prod database has no PostGIS: spatial queries must work without it
```

## Learn more

- [Install and set up](docs/setup.md): agents, settings, Obsidian, migrating
- [Reference](docs/reference.md): the vault, the note format, tools, skills, commands
- [How it works](docs/architecture.md): session start, the cleanup review, the checks, usage across machines
- [Compared with built-in memory](docs/comparison.md)
- [Contributing](CONTRIBUTING.md): building, testing, releasing

## Acknowledgements

Ideas borrowed from [Basic Memory](https://github.com/basicmachines-co/basic-memory), [IWE](https://github.com/iwe-org/iwe) and [okf-agent-memory](https://github.com/okf-memory/okf-agent-memory). Built with [Claude Code](https://claude.com/claude-code).

## License

[Apache-2.0](LICENSE)

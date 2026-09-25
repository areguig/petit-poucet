<p align="center"><img src="site/logo.svg" width="120" alt=""></p>

<h1 align="center">petit-poucet</h1>

<p align="center"><em>Leaves pebbles so your coding agents find their way back.</em></p>

<p align="center">
  <a href="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml"><img src="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/areguig/petit-poucet/releases/latest"><img src="https://img.shields.io/github/v/release/areguig/petit-poucet" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/areguig/petit-poucet" alt="Apache-2.0"></a>
</p>

In the French tale *Le Petit Poucet*, a boy drops white pebbles along the path so he can find his way home. **petit-poucet** does the same for AI coding agents: it keeps a small, curated memory of your rules, decisions and verified facts, so every new session in Claude Code, GitHub Copilot CLI or any MCP client starts where the last one left off.

## Why

Coding agents forget everything between sessions, and each tool keeps its own memory in its own format. petit-poucet gives them one shared memory that:

- **is plain Markdown**: a folder of notes you can read and edit in [Obsidian](https://obsidian.md) or any editor; no database, no cloud;
- **is curated, not recorded**: agents save a decision, a correction or a verified fact deliberately, with its source; nothing is captured from transcripts;
- **enforces its own rules**: note format, Index and links are checked by the server, not left to each agent's good will;
- **protects what you said**: rules you stated are never changed or deleted without your confirmation;
- **keeps history**: the vault is a git repository with a local commit after every change, so any agent edit can be inspected and undone;
- **is one small binary**: written in Rust, no runtime to install.

## What your agent sees

At the start of every session, a hook gives the agent the memory rules and the Index of what applies here: your preferences and the current repo's notes, one line each. Claude Code also shows you a pebble line:

```
🪨 petit-poucet · 20 notes loaded (preferences + chargepath-api)
```

```markdown
## Preferences (all repos)
- [[Preferences/commit-rules]] — commit locally per step, 1-2 line message, no Co-Authored-By, never push
- [[Preferences/never-read-secrets]] — use secret values only by piping them into commands, never print them

## Projects / chargepath-api
- [[Projects/chargepath-api/prod-db-no-postgis]] — the prod database has no PostGIS: spatial queries must work without it

Topics (not loaded: memory_search covers them, memory_index with `topic` lists one): homelab (8), work-mac (3)
```

The agent opens only the notes a task needs, saves new facts as it learns them, and from time to time a stop hook asks whether the session produced something worth remembering.

## How it works

```
Claude Code ─┐
Copilot CLI ─┼─ MCP tools + hooks ─▶ petit-poucet ─▶ ~/agent-memory/ (Markdown + git)
other MCP   ─┘                                       ├─ Index.md           generated
                                                     ├─ Preferences/       every repo, always loaded
                                                     ├─ Projects/<repo>/   loaded in that repo
                                                     └─ Topics/<topic>/    no repo (homelab, a server…), searched on demand
```

A project is recognised by its git remote first, then its folder name, so a teammate's checkout under another name still matches. Each note is one short fact:

```markdown
---
type: feedback            # user | feedback (a rule you stated) | project | reference
scope: all repos          # or the project or topic, from its folder
summary: commit locally per step, 1-2 line message, never push
created: 2026-09-14
tags: [agent-memory, git]
---

# Commit rules

Commit each finished step locally; never push.

**Why:** the user said so on 2026-09-14.
**How to apply:** after each step.
```

### Tools

| Tool | What it does |
|---|---|
| `memory_index` | Preferences, the current project and the topic names; with `topic`, that topic's notes |
| `memory_read` | One note |
| `memory_search` | Word search over preferences, the current project and all topics |
| `memory_save` | Create or update a note; the Index and a git commit follow |
| `memory_move` | Rename or re-scope a note, rewriting every link to it |
| `memory_delete` | Delete a note (with a reason) and unlink it everywhere |
| `memory_review` | One line per note with dates and read counts, for the cleanup agent |

### Skills

- **`migrate-memory`**: moves an agent's older file-based memory (`MEMORY.md` files, memory folders, memory sections of instruction files) into petit-poucet in one pass. It shows you the list first and saves only what you confirm; the old files are never touched.
- **`tidy-memory`**: a read-only subagent reviews the whole vault for duplicates, contradictions, stale or unused notes, and proposes fixes; your agent applies only what you confirm.

### Rules enforced in code

- A note has a valid type, a one-line summary, a source and the `agent-memory` tag; secrets (tokens, keys, passwords) are refused.
- Changing, moving or deleting a rule you stated (`feedback`) needs your confirmation.
- An agent can only update or delete a note it has read in its current version: an edit you made in Obsidian meanwhile is never overwritten.
- `Index.md` is generated from the notes, so it never lists a missing note or misses one.

## Install

Needs `git` and `curl` (macOS or Linux). The plugin downloads the petit-poucet binary for your platform on first use, checks its SHA-256 and caches it in `~/.cache/petit-poucet`.

**Claude Code**

```sh
claude plugin marketplace add https://github.com/areguig/petit-poucet
claude plugin install petit-poucet@petit-poucet
```

**GitHub Copilot CLI**

```sh
copilot plugin marketplace add areguig/petit-poucet
copilot plugin install petit-poucet@petit-poucet
```

This one install also serves Copilot in VS Code, IntelliJ and the GitHub Copilot app: they load the plugins Copilot CLI installed. The IDEs don't run plugin hooks, so there the `memory` skill loads your memory at the start of a task instead.

Then start a new session: the agent says memory has no vault yet and offers to create one in `~/agent-memory` (or wherever you prefer). Both agents share it. The vault path lives in `~/.config/petit-poucet/config.toml` (`PETIT_POUCET_VAULT` overrides it). Nothing is ever pushed from the vault.

Already keeping memory in files? Ask your agent to run the `migrate-memory` skill.

## Browse your memory in Obsidian

The vault is a plain folder of Markdown notes, so any editor works. For [Obsidian](https://obsidian.md): *Open folder as vault* and pick the vault folder. Obsidian is only a viewer: petit-poucet doesn't need it running, and your edits there are picked up on the next tool call. Don't edit `Index.md` by hand: it is regenerated from each note's `summary`.

## Command line

The same binary has a few commands for you (the release binaries are on the [Releases](https://github.com/areguig/petit-poucet/releases) page):

| Command | What it does |
|---|---|
| `petit-poucet check` | Validate the vault: frontmatter, summaries, scopes, links, secrets, Index |
| `petit-poucet init [path]` | Create a vault (default `~/agent-memory`) and the config file |
| `petit-poucet migrate` | Upgrade a hand-maintained vault: summaries from its old Index, full-path links, project identities, git |

## Developing

`main` is what plugin users get: it only ever holds released versions. Work happens on `dev`.

`plugin/` is the one plugin for every client: the launcher, the skills and `release.env` exist once. Claude Code reads `.claude-plugin/plugin.json`; Copilot reads `plugin.json`, which points at its own MCP config, hooks and agent in `copilot/`. Copilot's IDE hosts read the folder as a Claude plugin and use the Claude files.

Try a local build before any release: build it, then start an agent with the plugin folder from your checkout. Loaded from a checkout, the plugin runs the binary built there (`target/release/petit-poucet`) instead of downloading a release. Disable the installed plugin first so they don't both load:

```sh
cargo build --release
claude plugin disable petit-poucet@petit-poucet          # or: copilot plugin disable petit-poucet
claude --plugin-dir ./plugin                             # or: copilot --plugin-dir ./plugin
```

Re-enable the installed plugin afterwards (`claude plugin enable …`, `copilot plugin enable …`).

### Releasing

Only after the change was tried locally:

1. On `dev`, set the new version in `Cargo.toml`, `plugin/release.env`, `plugin/.claude-plugin/plugin.json`, `plugin/plugin.json` and `.github/plugin/marketplace.json` (`cargo test` fails until they all match).
2. Merge `dev` into `main`, tag `vX.Y.Z` on `main` and push both: the release workflow builds the four binaries and publishes them with their checksums.

## Acknowledgements

Ideas borrowed from [Basic Memory](https://github.com/basicmachines-co/basic-memory), [IWE](https://github.com/iwe-org/iwe) and [okf-agent-memory](https://github.com/okf-memory/okf-agent-memory).

petit-poucet was built with [Claude Code](https://claude.com/claude-code).

## License

[Apache-2.0](LICENSE)

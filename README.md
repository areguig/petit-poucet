<p align="center"><img src="site/logo.svg" width="120" alt=""></p>

<h1 align="center">petit-poucet</h1>

<p align="center"><em>Leaves pebbles so your coding agents find their way back.</em></p>

<p align="center">
  <a href="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml"><img src="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/areguig/petit-poucet/releases/latest"><img src="https://img.shields.io/github/v/release/areguig/petit-poucet" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/areguig/petit-poucet" alt="Apache-2.0"></a>
</p>

In the French tale *Le Petit Poucet*, a boy drops white pebbles along the path so he can find his way home. **petit-poucet** does the same for AI coding agents: it keeps a small, curated memory of your rules, decisions and verified facts, so every new session in Claude Code, GitHub Copilot CLI or any MCP client starts where the last one left off.

Planned: [Codex](https://github.com/areguig/petit-poucet/issues/9), [OpenCode](https://github.com/areguig/petit-poucet/issues/10) and [other popular agents](https://github.com/areguig/petit-poucet/issues/11). A 👍 on the issue for your agent helps decide what comes first.

## Why

Coding agents forget everything between sessions, and each tool keeps its own memory in its own format. petit-poucet gives them one shared memory that:

- **is plain Markdown**: a folder of notes you can read and edit in [Obsidian](https://obsidian.md) or any editor; no database, no cloud;
- **is curated, not recorded**: agents save a decision, a correction or a verified fact deliberately, with its source; nothing is captured from transcripts;
- **enforces its own rules**: note format, Index and links are checked by the server, not left to each agent's good will;
- **protects what you said**: rules you stated are never changed or deleted without your confirmation;
- **keeps history**: the vault is a git repository with a local commit after every change, so any agent edit can be inspected and undone;
- **is one small binary**: written in Rust, no runtime to install.

## Compared with built-in agent memory

Claude Code, Copilot and Codex now keep memories of their own, and that's useful. petit-poucet differs in where the memory lives, who can read it, and how much say you have in it:

| | Claude Code auto memory | GitHub Copilot Memory | Codex memories | petit-poucet |
|---|---|---|---|---|
| Where it lives | `~/.claude/projects/<repo>/memory/`, on this machine | Stored by GitHub (in VS Code: local memory files) | Generated files in `~/.codex/memories/` | A folder you choose: plain Markdown, Obsidian-compatible |
| Other agents | Claude Code only | Copilot only | Codex only | Every agent you plug in: Claude Code, Copilot, any MCP client |
| Scope | One repo; rules for all repos go in `CLAUDE.md`, by hand | One repo | Global | Preferences for every repo, notes per repo (matched by git remote), topics tied to no repo |
| How notes are made | Claude decides what to save | The agent saves while working | Summarised in the background from past chats | Saved on purpose, one fact per note, each with its source |
| Your say | Edit the files yourself | Review and delete in repository settings | Docs advise against editing them by hand | Rules you stated change only with your confirmation; your edits are never overwritten |
| History | Last-modified time | No version history | No version history | A git commit for every change |
| Lifetime | Until edited; only the first 200 lines of the index load | Unused memories expire after 28 days | Off by default; managed by Codex | Until you, or a cleanup you confirmed, remove it |

Sources, October 2026: [Claude Code memory](https://code.claude.com/docs/en/memory), [Copilot Memory](https://github.blog/changelog/2026-03-04-copilot-memory-now-on-by-default-for-pro-and-pro-users-in-public-preview/), [memory in VS Code](https://code.visualstudio.com/docs/copilot/agents/memory), [Codex memories](https://developers.openai.com/codex/memories). These features change often.

If you use one agent and are happy with what it remembers, its built-in memory may be all you need. petit-poucet helps when you switch between agents, want one place to read and correct what they know, or need rules that don't drift, expire or get rewritten silently. Already have built-in memory? The `migrate-memory` skill moves it in.

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
| `memory_search` | Word search over preferences, the current project and all topics (every note when the folder is no known project) |
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

**Codex**

Install the binary (below), then run `petit-poucet setup`: it adds the MCP server to `~/.codex/config.toml` and the session-start and stop hooks to `~/.codex/hooks.json`, keeping everything else in those files (and a `.petit-poucet.bak` copy of what it changed). Open Codex once and trust the new hooks with `/hooks`. `petit-poucet setup --uninstall` takes them out again.

**The binary on its own**

```sh
curl -fsSL https://raw.githubusercontent.com/areguig/petit-poucet/main/install.sh | sh
```

This installs the latest release in `~/.local/bin` (`PETIT_POUCET_INSTALL_DIR` changes the folder, `PETIT_POUCET_VERSION` picks a version) after checking its SHA-256; run it again to upgrade. Then run `petit-poucet setup`: it creates your vault if you have none and tells you, agent by agent, what's set up. With the binary on your PATH you also get the [command line](#command-line), and the plugins use it instead of downloading their own copy whenever it is the version they expect.

## Browse your memory in Obsidian

The vault is a plain folder of Markdown notes, so any editor works. For [Obsidian](https://obsidian.md): *Open folder as vault* and pick the vault folder. Obsidian is only a viewer: petit-poucet doesn't need it running, and your edits there are picked up on the next tool call. Don't edit `Index.md` by hand: it is regenerated from each note's `summary`.

## Command line

The same binary has a few commands for you (install it with the [install script](#install), or take a binary from the [Releases](https://github.com/areguig/petit-poucet/releases) page):

| Command | What it does |
|---|---|
| `petit-poucet setup` | Set up memory for every agent on this machine and create the vault if there's none; `--check` reports what's missing, `--uninstall` removes petit-poucet from the agents (never the vault), `--agent <name>` limits it to one |
| `petit-poucet check` | Validate the vault: frontmatter, summaries, scopes, links, secrets, Index |
| `petit-poucet init [path]` | Create a vault (default `~/agent-memory`) and the config file |
| `petit-poucet migrate` | Upgrade a hand-maintained vault: summaries from its old Index, full-path links, project identities, git |

## Developing

`main` is what plugin users get: it only ever holds released versions. Each version is built on its own branch cut from `main` after the previous release (e.g. `v0.3`): pull requests for that version target it. Documentation-only changes (README, `site/`, `docs/`) go to `main` directly, as long as they describe the released version.

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

1. On the version branch, set the new version in `Cargo.toml`, `plugin/release.env`, `plugin/.claude-plugin/plugin.json`, `plugin/plugin.json` and `.github/plugin/marketplace.json`, and write the release notes in `docs/releases/vX.Y.Z.md` (`cargo test` fails until they all match and the notes exist).
2. Tag `vX.Y.Z` on the version branch and push the tag: the release workflow builds the four binaries and publishes them with their checksums and the notes. Once the release is out, merge the version branch into `main` with a pull request that lists `Closes #…` for its issues. In that order, plugin users never get a `release.env` whose binaries aren't published yet.
3. Delete the version branch, after moving its still-open pull requests to the next version's branch. A fix needed before the next version is ready gets its own patch branch from `main`, named `vX.Y.Z-fixes` (e.g. `v0.3.1-fixes`: the tag `v0.3.1` must not share its name); once released, merge `main` into the branch in progress.

## Acknowledgements

Ideas borrowed from [Basic Memory](https://github.com/basicmachines-co/basic-memory), [IWE](https://github.com/iwe-org/iwe) and [okf-agent-memory](https://github.com/okf-memory/okf-agent-memory).

petit-poucet was built with [Claude Code](https://claude.com/claude-code).

## License

[Apache-2.0](LICENSE)

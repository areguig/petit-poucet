<p align="center"><img src="site/logo.svg" width="120" alt=""></p>

<h1 align="center">petit-poucet</h1>

<p align="center"><em>Leaves pebbles so your coding agents find their way back.</em></p>

<p align="center">
  <a href="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml"><img src="https://github.com/areguig/petit-poucet/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/areguig/petit-poucet/releases/latest"><img src="https://img.shields.io/github/v/release/areguig/petit-poucet" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/github/license/areguig/petit-poucet" alt="Apache-2.0"></a>
</p>

In the French tale *Le Petit Poucet*, a boy drops white pebbles along the path so he can find his way home. **petit-poucet** does the same for AI coding agents: it keeps a small, curated memory of your rules, decisions and verified facts, and every agent you switch between starts each session with it.

One binary (an MCP memory server and its hooks), one command to wire it into **Claude Code, GitHub Copilot (CLI, VS Code, IntelliJ, the Copilot app), Codex, Cursor and Antigravity CLI**, on macOS, Linux and Windows. Any other MCP client can use its tools. Next: [OpenCode](https://github.com/areguig/petit-poucet/issues/10); a 👍 on the issue for your agent helps decide what comes after.

## Why

Coding agents forget everything between sessions, and each tool keeps its own memory in its own format: what you taught Claude Code on Monday, Codex doesn't know on Tuesday. petit-poucet gives them one shared memory that:

- **follows you across agents**: `petit-poucet setup` wires the same vault into every agent it finds, so switching tools doesn't mean teaching them again;
- **is there from the first message**: a session-start hook hands the agent your rules and the Index, so memory doesn't depend on the model deciding to look it up;
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
| Other agents | Claude Code only | Copilot only | Codex only | Claude Code, Copilot, Codex, Cursor, Antigravity CLI, any MCP client |
| Scope | One repo; rules for all repos go in `CLAUDE.md`, by hand | One repo | Global | Preferences for every repo, notes per repo (matched by git remote), topics tied to no repo |
| How notes are made | Claude decides what to save | The agent saves while working | Summarised in the background from past chats | Saved on purpose, one fact per note, each with its source |
| Your say | Edit the files yourself | Review and delete in repository settings | Docs advise against editing them by hand | Rules you stated change only with your confirmation; your edits are never overwritten |
| History | Last-modified time | No version history | No version history | A git commit for every change |
| Lifetime | Until edited; only the first 200 lines of the index load | Unused memories expire after 28 days | Off by default; managed by Codex | Until you, or a cleanup you confirmed, remove it |

Sources, October 2026: [Claude Code memory](https://code.claude.com/docs/en/memory), [Copilot Memory](https://github.blog/changelog/2026-03-04-copilot-memory-now-on-by-default-for-pro-and-pro-users-in-public-preview/), [memory in VS Code](https://code.visualstudio.com/docs/copilot/agents/memory), [Codex memories](https://developers.openai.com/codex/memories). These features change often.

If you use one agent and are happy with what it remembers, its built-in memory may be all you need. petit-poucet helps when you switch between agents, want one place to read and correct what they know, or need rules that don't drift, expire or get rewritten silently. Already have built-in memory? The `migrate-memory` skill moves it in.

## What your agent sees

At the start of every session, a hook gives the agent the memory rules and the Index of what applies here: your preferences and the current repo's notes, one line each. Claude Code and Codex also show you a pebble line:

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
Copilot     ─┤
Codex       ─┼─ MCP tools + hooks ─▶ petit-poucet ─▶ ~/agent-memory/ (Markdown + git)
Cursor      ─┤                                       ├─ Index.md           generated
Antigravity ─┘                                       ├─ Preferences/       every repo, always loaded
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
| `memory_review` | For the cleanup agent: the notes changed since the last cleanup (all of them the first time) and the problems found in the whole vault, in pages small enough for every agent |

### Skills

- **`memory`**: tells the agent to load your memory at the start of a task, where hooks don't run (Copilot in IntelliJ).
- **`migrate-memory`**: moves an agent's older file-based memory (`MEMORY.md` files, memory folders, memory sections of instruction files) into petit-poucet in one pass. It shows you the list first and saves only what you confirm; the old files are never touched.
- **`tidy-memory`**: a read-only subagent reviews the whole vault for duplicates, contradictions, stale or unused notes, and proposes fixes; your agent applies only what you confirm.

### Rules enforced in code

- A note has a valid type, a one-line summary, a source and the `agent-memory` tag; secrets (tokens, keys, passwords) are refused.
- Changing, moving or deleting a rule you stated (`feedback`) needs your confirmation.
- An agent can only update or delete a note it has read in its current version: an edit you made in Obsidian meanwhile is never overwritten.
- `Index.md` is generated from the notes, so it never lists a missing note or misses one.

## Install

Needs `git`, and `curl` on macOS or Linux (Windows 10 and later ship it). On macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/areguig/petit-poucet/main/install.sh | sh
petit-poucet setup
```

On Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/areguig/petit-poucet/main/install.ps1 | iex
petit-poucet setup
```

The script installs the latest release in `~/.local/bin` after checking its SHA-256 (`PETIT_POUCET_INSTALL_DIR` changes the folder, `PETIT_POUCET_VERSION` picks a version); run it again to upgrade. On Windows it also adds that folder to your user `Path` (`PETIT_POUCET_NO_MODIFY_PATH=1` leaves it alone): open a new terminal before `petit-poucet setup`. Once a day, session start checks for a new release in the background and tells you when one is out (`PETIT_POUCET_NO_UPDATE_CHECK=1` turns the check off). `petit-poucet setup` creates your vault in `~/agent-memory` if you have none, then wires every agent it finds: the MCP server, the session-start and end-of-turn hooks, the `memory`, `migrate-memory` and `tidy-memory` skills, and the `memory-cleanup` subagent that `tidy-memory` hands the review to. It writes each agent's user-level config, keeps everything else in those files and leaves a `.petit-poucet.bak` copy of what it changed. `setup --check` says what's missing, `setup --uninstall` takes it all out again (your vault stays), and `--agent claude` (or `copilot`, `codex`, `cursor`, `antigravity`) limits either to one agent.

| Agent | What `setup` writes | Then |
|---|---|---|
| Claude Code | `~/.claude.json`, `~/.claude/settings.json`, `~/.claude/skills`, `~/.claude/agents` | Restart Claude Code. |
| GitHub Copilot: CLI, VS Code, IntelliJ, the Copilot app | `~/.copilot/mcp-config.json`, `~/.copilot/hooks/petit-poucet.json`, `~/.agents/skills`, `~/.copilot/agents` | All of them read `~/.copilot`. Where hooks don't run (IntelliJ), the `memory` skill loads your memory at the start of a task. |
| Codex | `~/.codex/config.toml`, `~/.codex/hooks.json`, `~/.agents/skills`, `~/.codex/agents` | Open Codex once and trust the new hooks with `/hooks`. |
| Cursor | `~/.cursor/mcp.json`, `~/.cursor/hooks.json`, `~/.agents/skills`, `~/.cursor/agents` | Restart Cursor. Cloud agents don't read user-level hooks: there memory loads through the MCP server's instructions. |
| Antigravity CLI | `~/.gemini/config/`: `mcp_config.json`, `hooks.json`, `skills`, `agents` | Memory reaches the model before each of its calls. Gemini CLI isn't supported: it no longer serves personal Google accounts, and Antigravity CLI replaces it. |

Coming from the 0.2 plugin for Claude Code or Copilot? `petit-poucet setup` removes it with the agent's own command, so its hooks don't run twice.

Every agent shares the one vault. Its path lives in `~/.config/petit-poucet/config.toml` (`PETIT_POUCET_VAULT` overrides it). Nothing is ever pushed from the vault. Already keeping memory in files? Ask your agent to run the `migrate-memory` skill.

## Browse your memory in Obsidian

The vault is a plain folder of Markdown notes, so any editor works. For [Obsidian](https://obsidian.md): *Open folder as vault* and pick the vault folder. Obsidian is only a viewer: petit-poucet doesn't need it running, and your edits there are picked up on the next tool call. Don't edit `Index.md` by hand: it is regenerated from each note's `summary`.

## Command line

The same binary has a few commands for you (install it with the [install script](#install), or take a binary from the [Releases](https://github.com/areguig/petit-poucet/releases) page):

| Command | What it does |
|---|---|
| `petit-poucet setup` | Set up memory for every agent on this machine and create the vault if there's none; `--check` reports what's missing, `--uninstall` removes petit-poucet from the agents (never the vault), `--agent <name>` limits it to one |
| `petit-poucet check` | Validate the vault: frontmatter, summaries, scopes, links, secrets, near-duplicates, Index; and report the latest release |
| `petit-poucet init [path]` | Create a vault (default `~/agent-memory`) and the config file |
| `petit-poucet migrate` | Upgrade a hand-maintained vault: summaries from its old Index, full-path links, project identities, git |

## Developing

`main` is what users get (the install script and the site come from it): it only ever holds released versions. Each version is built on its own branch cut from `main` after the previous release (e.g. `v0.3`): pull requests for that version target it. Documentation-only changes (README, `site/`, `docs/`) go to `main` directly, as long as they describe the released version.

The skills (`skills/`) and the `memory-cleanup` prompt (`agents/`) are embedded in the binary: `setup` writes them for each agent in its own format.

Each supported agent has a script in `tests/agents/` that installs petit-poucet into that agent's real CLI, in a throwaway HOME so your own config is never touched: `cargo build --release && npm ci --prefix tests/agents && sh tests/agents/codex.sh` (or `claude.sh`, `copilot.sh`, `cursor.sh`, `antigravity.sh`). The Agents workflow runs them on Linux, macOS and Windows with each agent's latest release. Except Cursor's, which can't use another model, each script then runs a three-turn session against a mock model ([aimock](https://github.com/CopilotKit/llmock), in `tests/agents/model.mjs`) and checks every request the agent sent: the memory is there once, petit-poucet's tools and skills are offered, and the save reminder comes once, after the third turn.

Try a local build before any release: `cargo build --release && ./target/release/petit-poucet setup` points every agent at that build. Run `setup` from the installed binary afterwards to point them back.

### Releasing

Only after the change was tried locally:

1. On the version branch, set the new version in `Cargo.toml` and finish the release notes in `docs/releases/vX.Y.Z.md` (`cargo test` fails until the notes for the crate's version exist).
2. Merge the version branch into `main` with a pull request that lists `Closes #…` for its issues, then tag `vX.Y.Z` on `main` and push the tag: the release workflow builds the six binaries (macOS, Linux and Windows, each on x64 and ARM) and publishes them with their checksums and the notes. Until it's done (a few minutes), `main` describes a version the install scripts can't download yet.
3. Delete the version branch, after moving its still-open pull requests to the next version's branch. A fix needed before the next version is ready gets its own patch branch from `main`, named `vX.Y.Z-fixes` (e.g. `v0.3.1-fixes`: the tag `v0.3.1` must not share its name); once released, merge `main` into the branch in progress.

## Acknowledgements

Ideas borrowed from [Basic Memory](https://github.com/basicmachines-co/basic-memory), [IWE](https://github.com/iwe-org/iwe) and [okf-agent-memory](https://github.com/okf-memory/okf-agent-memory).

petit-poucet was built with [Claude Code](https://claude.com/claude-code).

## License

[Apache-2.0](LICENSE)

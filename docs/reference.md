# Reference

## The vault

```
~/agent-memory/
├─ Index.md            generated from the notes' summaries
├─ Preferences/        your rules for every repo, loaded in every session
├─ Projects/<repo>/    a repo's notes, loaded in that repo
└─ Topics/<topic>/     knowledge tied to no repo (a homelab, a server…), searched on demand
```

A project is recognised by its git remote first, then its folder name, so a checkout under another name still matches. [How it works](architecture.md) lists the other files petit-poucet keeps there.

## A note

One short fact per note:

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

## Tools (MCP)

| Tool | What it does |
|---|---|
| `memory_index` | Preferences, the current project and the topic names; with `topic`, that topic's notes |
| `memory_read` | One note |
| `memory_search` | Word search over preferences, the current project and all topics (every note when the folder is no known project) |
| `memory_save` | Create or update a note; the Index and a git commit follow |
| `memory_move` | Rename or re-scope a note, rewriting every link to it |
| `memory_delete` | Delete a note (with a reason) and unlink it everywhere |
| `memory_review` | For the cleanup agent: the notes to review and the problems found in the whole vault, in pages small enough for every agent ([how](architecture.md#the-cleanup-review-memory_review)) |

## Skills

- **`memory`**: tells the agent to load your memory at the start of a task, where hooks don't run (Copilot in IntelliJ).
- **`migrate-memory`**: moves what agents remember elsewhere (instruction files, memory folders, agents' own memories) into petit-poucet in one pass. It shows you the list first and saves only what you confirm; the old files are never touched.
- **`tidy-memory`**: a read-only subagent reviews the vault for duplicates, contradictions, stale, unused or badly shaped notes, and proposes fixes; your agent applies only what you confirm. "Review my whole memory" reviews every note, whatever the vault's size.

## Commands

| Command | What it does |
|---|---|
| `petit-poucet setup` | Set up memory for every agent on this machine and create the vault if there's none; `--check` reports what's missing, `--uninstall` removes petit-poucet from the agents (never the vault), `--agent <name>` limits it to one |
| `petit-poucet check` | Validate the vault: frontmatter, summaries and their length, scopes, links, secrets, near-duplicates, paths gone from a project's checkout, the Index and its size per session; and report the latest release |
| `petit-poucet init [path]` | Create a vault (default `~/agent-memory`) and the config file |
| `petit-poucet migrate` | Upgrade a hand-maintained vault: summaries from its old Index, full-path links, project identities, git |

## Rules enforced in code

- A note has a valid type, a one-line summary, a source and the `agent-memory` tag; secrets (tokens, keys, passwords) are refused.
- Changing, moving or deleting a rule you stated (`feedback`) needs your confirmation.
- An agent can only update or delete a note it has read in its current version: an edit you made in Obsidian meanwhile is never overwritten.
- `Index.md` is generated from the notes, so it never lists a missing note or misses one.

# Reference

## The vault

```
~/agent-memory/
├─ Index.md            generated from the notes' summaries
├─ Preferences/        your rules for every repo, loaded in every session (situational ones are searched)
├─ Projects/<repo>/    a repo's notes, loaded in that repo
└─ Topics/<topic>/     knowledge tied to no repo (a homelab, a server…), searched on demand
```

A project is recognised by its git remote first, then its folder name, so a checkout under another name still matches. [How it works](architecture.md) lists the other files petit-poucet keeps there.

A remote on a non-default port, such as `ssh://git@host:2222/team/app.git`, is kept with its port, so it never matches the same path on another port. Before 0.7.1 the port was dropped: if a repo on such a port no longer loads its project, open the project's `_project.md` and replace its `host/team/app` remote with the full URL from `git remote -v`. Do it only for the project that belongs to that port; if repos on two ports had ended up in one project, move the other repo's notes to a project of its own.

## A note

One short fact per note:

```markdown
---
type: feedback            # user | feedback (a rule you stated) | project | reference
scope: all repos          # or the project or topic, from its folder
summary: commit locally per step, 1-2 line message, never push
created: 2026-09-14
tags: [agent-memory, git]
applies_when: editing Rust code   # preferences only, optional: omit it when the rule always applies
---

# Commit rules

Commit each finished step locally; never push.

**Why:** the user said so on 2026-09-14.
**How to apply:** after each step.
```

A preference without `applies_when` loads in every session. One with it is situational: it stays cross-project but is not loaded at session start. The session says how many there are, `memory_search` finds them (it searches the `applies_when` line too) and shows `(when: …)` after each hit, and `memory_index` and `Index.md` list them with the same suffix. `memory_save` takes `applies_when` on a preference (an empty string clears it; omitting it keeps it) and refuses it on project and topic notes. `tidy-memory` proposes, batch by batch, which preferences to keep always-on, make situational or move to a project or topic; nothing changes until you confirm.

## Tools (MCP)

| Tool | What it does |
|---|---|
| `memory_index` | Preferences, the current project and the topic names; with `topic`, that topic's notes |
| `memory_read` | One note, or a sync conflict copy of one; the note by its name in the Index (`commit-rules`) or its full path. `memory_save`, `memory_move` and `memory_delete` take the same |
| `memory_search` | Word search over preferences, the current project and all topics (every note when the folder is no known project): notes matching the most words first, rare words before common ones, and none matching under half the words when others match more |
| `memory_save` | Create or update a note, a preference with an optional `applies_when`; the Index and a git commit follow |
| `memory_move` | Rename or re-scope a note, rewriting every link to it |
| `memory_delete` | Delete a note (with a reason) and unlink it everywhere, or a sync conflict copy once merged |
| `memory_review` | For the cleanup agent: the notes to review and the problems found in the whole vault, in pages small enough for every agent ([how](architecture.md#the-cleanup-review-memory_review)) |

## Skills

- **`memory`**: tells the agent to load your memory at the start of a task, where hooks don't run (Copilot in IntelliJ).
- **`migrate-memory`**: moves what agents remember elsewhere (instruction files, memory folders, agents' own memories) into petit-poucet in one pass. It shows you the list first and saves only what you confirm; the old files are never touched.
- **`tidy-memory`**: a read-only subagent reviews the vault for duplicates, contradictions, stale, unused or badly shaped notes, and proposes fixes; your agent applies only what you confirm. "Review my whole memory" reviews every note, whatever the vault's size.

## Commands

| Command | What it does |
|---|---|
| `petit-poucet setup` | Set up memory for every agent on this machine and create the vault if there's none; `--check` reports what's missing, `--uninstall` removes petit-poucet from the agents (never the vault), `--agent <name>` limits it to one |
| `petit-poucet update` | Install the latest release over the binary the install script placed, after checking its SHA-256, then run `setup` with it |
| `petit-poucet sync [device id]` | Share the vault through the Syncthing installed on this machine; with another machine's device ID, pair with it. Lists the machines it syncs with, and this machine's own `.stignore` lines. Without Syncthing, says how to install and start it ([guide](syncthing.md)) |
| `petit-poucet check` | Validate the vault: frontmatter, summaries and their length, scopes, links, secrets, near-duplicates, file names whose title changed, paths gone from a project's checkout, the Index and its size per session; and report the latest release |
| `petit-poucet init [path]` | Create a vault (default `~/agent-memory`) and the config file |
| `petit-poucet migrate` | Upgrade a hand-maintained vault: summaries from its old Index, full-path links, project identities, git |

## Rules enforced in code

- A note has a valid type, a one-line summary, a source and the `agent-memory` tag; secrets (tokens, keys, passwords) are refused.
- Changing, moving or deleting a rule you stated (`feedback`) needs your confirmation.
- An agent can only update or delete a note it has read in its current version: an edit you made in Obsidian meanwhile is never overwritten.
- `Index.md` is generated from the notes, so it never lists a missing note or misses one.

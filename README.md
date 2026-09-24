# petit-poucet

> Leaves pebbles so your coding agents find their way back.

In the French tale *Le Petit Poucet*, a boy drops white pebbles along the path so he can find his way home. **petit-poucet** does the same for AI coding agents: it keeps a small, curated memory of your rules, decisions and verified facts, so every new session — in Claude Code, GitHub Copilot, or any MCP client — starts where the last one left off.

**Status:** early development, not usable yet.

## Why

Coding agents forget everything between sessions, and each tool keeps its own memory in its own format. petit-poucet gives them one shared memory that:

- **is plain Markdown** — a folder of notes you can read and edit in [Obsidian](https://obsidian.md) or any editor; no database, no cloud;
- **is curated, not recorded** — agents save a decision, a correction or a verified fact deliberately, with its source; nothing is captured automatically from transcripts;
- **enforces its own rules** — note format, index and links are checked by the server, not left to each agent's good will;
- **protects what you said** — rules you stated are never changed or deleted without your confirmation;
- **keeps history** — the vault is a git repository with a local commit after every change, so any agent edit can be inspected and undone;
- **is one small binary** — written in Rust, no runtime to install.

## How it works

```
Claude Code ─┐
Copilot CLI ─┼─ MCP tools + hooks ─▶ petit-poucet ─▶ vault/ (Markdown + git)
other MCP   ─┘                                       ├─ Index.md           generated
                                                     ├─ Preferences/       every repo
                                                     └─ Projects/<repo>/   one repo
```

- **Session start:** a hook injects the index of your memory (preferences plus the current repo's notes) into the agent's context.
- **During the session:** the agent opens only the notes it needs and saves new ones through a few MCP tools (`memory_index`, `memory_read`, `memory_save`, `memory_delete`, `memory_move`, `memory_search`).
- **From time to time:** a stop hook asks the agent whether the session produced something worth remembering — and to say nothing if not.

## Install

Not released yet. Planned: a plugin for Claude Code and for Copilot CLI that downloads the right binary for your OS (macOS and Linux first, Windows later).

## Browse your memory in Obsidian

The vault is a plain folder of Markdown notes, so any editor works. For [Obsidian](https://obsidian.md): *Open folder as vault* and pick the vault folder (the `vault` path in `~/.config/petit-poucet/config.toml`). Obsidian is only a viewer: petit-poucet doesn't need it running, and edits you make there are picked up on the next tool call. Don't edit `Index.md` by hand: it is regenerated from each note's `summary`; run `petit-poucet check` after editing notes yourself.

## Acknowledgements

Ideas borrowed from [Basic Memory](https://github.com/basicmachines-co/basic-memory), [IWE](https://github.com/iwe-org/iwe) and [okf-agent-memory](https://github.com/okf-memory/okf-agent-memory).

## License

To be decided before the first public release.

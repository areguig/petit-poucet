# petit-poucet

> Leaves pebbles so your coding agents find their way back.

In the French tale *Le Petit Poucet*, a boy drops white pebbles along the path so he can find his way home. **petit-poucet** does the same for AI coding agents: it keeps a small, curated memory of your rules, decisions and verified facts, so every new session — in Claude Code, GitHub Copilot, or any MCP client — starts where the last one left off.

**Status:** v0.1: works with Claude Code and GitHub Copilot CLI on macOS and Linux.

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
                                                     ├─ Projects/<repo>/   one repo
                                                     └─ Topics/<topic>/    no repo (homelab, …), searched on demand
```

- **Session start:** a hook injects the index of your memory (preferences plus the current repo's notes) into the agent's context.
- **During the session:** the agent opens only the notes it needs and saves new ones through a few MCP tools (`memory_index`, `memory_read`, `memory_save`, `memory_delete`, `memory_move`, `memory_search`).
- **From time to time:** a stop hook asks the agent whether the session produced something worth remembering — and to say nothing if not.

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

Then start a new session: the agent says memory has no vault yet and offers to create one in `~/agent-memory` (or wherever you prefer). The vault path lives in `~/.config/petit-poucet/config.toml`; both agents share it. The vault is a git repository with a local commit after every change: nothing is ever pushed.

## Browse your memory in Obsidian

The vault is a plain folder of Markdown notes, so any editor works. For [Obsidian](https://obsidian.md): *Open folder as vault* and pick the vault folder (the `vault` path in `~/.config/petit-poucet/config.toml`). Obsidian is only a viewer: petit-poucet doesn't need it running, and edits you make there are picked up on the next tool call. Don't edit `Index.md` by hand: it is regenerated from each note's `summary`; run `petit-poucet check` after editing notes yourself.

## Acknowledgements

Ideas borrowed from [Basic Memory](https://github.com/basicmachines-co/basic-memory), [IWE](https://github.com/iwe-org/iwe) and [okf-agent-memory](https://github.com/okf-memory/okf-agent-memory).

## Developing

`main` is what plugin users get: it only ever holds released versions. Work happens on `dev`.

Try a local build before any release: build it, then start an agent with the plugin folder from your checkout. Loaded from a checkout, the plugin runs the binary built there (`target/release/petit-poucet`) instead of downloading a release. Disable the installed plugin first so they don't both load:

```sh
cargo build --release
claude plugin disable petit-poucet@petit-poucet          # or: copilot plugin disable petit-poucet
claude --plugin-dir ./plugin                             # or: copilot --plugin-dir ./plugin
```

Re-enable the installed plugin afterwards (`claude plugin enable …`, `copilot plugin enable …`).

## Releasing

Only after the change was tried locally:

1. On `dev`, set the new version in `Cargo.toml`, `plugin/release.env`, both `plugin.json` files and `.github/plugin/marketplace.json` (`cargo test` fails until they all match).
2. Merge `dev` into `main`, tag `vX.Y.Z` on `main` and push both: the release workflow builds the four binaries and publishes them with their checksums.

## License

[Apache-2.0](LICENSE)

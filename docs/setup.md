# Install and set up

## Install

Needs `git`, and `curl` on macOS or Linux (Windows 10 and later ship it).

```sh
# macOS, Linux
curl -fsSL https://raw.githubusercontent.com/areguig/petit-poucet/main/install.sh | sh
```

```powershell
# Windows, in PowerShell
irm https://raw.githubusercontent.com/areguig/petit-poucet/main/install.ps1 | iex
```

The script installs the latest release in `~/.local/bin` after checking its SHA-256. Run it again to upgrade.

| Variable | Effect |
|---|---|
| `PETIT_POUCET_INSTALL_DIR` | install somewhere else |
| `PETIT_POUCET_VERSION` | install a given version |
| `PETIT_POUCET_NO_MODIFY_PATH=1` | Windows: leave your user `Path` alone (by default the folder is added; open a new terminal afterwards) |
| `PETIT_POUCET_NO_UPDATE_CHECK=1` | no daily check for a new release (by default, session start checks in the background and tells you when one is out) |

You can also take a binary from the [Releases](https://github.com/areguig/petit-poucet/releases) page.

## Set up your agents

```sh
petit-poucet setup
```

`setup` creates your vault in `~/agent-memory` if you have none, then wires every agent it finds: the MCP server, the session-start and end-of-turn hooks, the `memory`, `migrate-memory` and `tidy-memory` skills, and the `memory-cleanup` subagent. It writes each agent's user-level config, keeps everything else in those files, and leaves a `.petit-poucet.bak` copy of what it changed.

- `setup --check` says what's missing.
- `setup --uninstall` takes it all out again; your vault stays.
- `--agent claude` (or `copilot`, `codex`, `cursor`, `antigravity`, `opencode`) limits either to one agent.

| Agent | What `setup` writes | Then |
|---|---|---|
| Claude Code | `~/.claude.json`, `~/.claude/settings.json`, `~/.claude/skills`, `~/.claude/agents` | Restart Claude Code. |
| GitHub Copilot: CLI, VS Code, IntelliJ, the Copilot app | `~/.copilot/mcp-config.json`, `~/.copilot/hooks/petit-poucet.json`, `~/.agents/skills`, `~/.copilot/agents` | All of them read `~/.copilot`. Where hooks don't run (IntelliJ), the `memory` skill loads your memory at the start of a task. |
| Codex | `~/.codex/config.toml`, `~/.codex/hooks.json`, `~/.agents/skills`, `~/.codex/agents` | Open Codex once and trust the new hooks with `/hooks`. |
| Cursor | `~/.cursor/mcp.json`, `~/.cursor/hooks.json`, `~/.agents/skills`, `~/.cursor/agents` | Restart Cursor. Cloud agents don't read user-level hooks: there memory loads through the MCP server's instructions. |
| Antigravity CLI | `~/.gemini/config/`: `mcp_config.json`, `hooks.json`, `skills`, `agents` | Memory reaches the model before each of its calls. Gemini CLI isn't supported: it no longer serves personal Google accounts, and Antigravity CLI replaces it. |
| OpenCode 2 | `~/.config/opencode/`: `opencode.json`, `plugins/petit-poucet.js`, `agents`; `~/.agents/skills` | Restart OpenCode. OpenCode 1.x isn't supported: install OpenCode 2 (`curl -fsSL https://opencode.ai/v2/install \| bash`). OpenCode has no hooks: the plugin adds your memory to the system prompt of each model call and sends the save reminder when a session's turn ends. |

Coming from the 0.2 plugin for Claude Code or Copilot? `setup` removes it with the agent's own command, so its hooks don't run twice.

Any other MCP client can use the tools: point it at `petit-poucet serve`.

## Already keeping memory elsewhere?

Ask your agent to run the `migrate-memory` skill. It looks for every supported agent's instruction files and own memory, shows you one proposed note per fact, and saves only what you confirm. The old files are never touched.

## Settings

Every agent shares the one vault. Its path lives in `~/.config/petit-poucet/config.toml`; `PETIT_POUCET_VAULT` overrides it. The file holds every setting with its value. A setting added by a newer version is written in the first time the file is read, keeping your own lines and comments.

| Setting | Default | What it does |
|---|---|---|
| `vault` | `~/agent-memory` | the vault folder |
| `git_autocommit` | `true` | commit every change in the vault (nothing is ever pushed) |
| `full_review_max_notes` | 300 | up to this many notes, every cleanup reviews every note |
| `review_max_pages` | 10 | above that, how many pages a cleanup review fills |
| `active_days` | 30 | a folder used within this many days comes early in a big vault's review |
| `unused_days` | 90 | a note not read, written or updated for this long is listed as unused |
| `summary_max_chars` | 200 | `check` warns about longer summaries |
| `index_max_notes` | 100 | `check` and session start warn when a session loads more notes |
| `cleanup_reminder_notes` | 30 | session start reminds of a cleanup after this many changed notes |
| `cleanup_reminder_days` | 30 | … or this many days since the last one |

## Browse your memory in Obsidian

The vault is a plain folder of Markdown notes, so any editor works. In [Obsidian](https://obsidian.md): *Open folder as vault* and pick the vault folder. petit-poucet doesn't need Obsidian running, and your edits there are picked up on the next tool call. Don't edit `Index.md` by hand: it is regenerated from each note's `summary`.

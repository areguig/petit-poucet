# Compared with built-in agent memory

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

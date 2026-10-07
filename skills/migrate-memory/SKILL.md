---
name: migrate-memory
description: "Move what agents remember elsewhere (MEMORY.md, memory folders, agents' own memories, memory sections of CLAUDE.md, AGENTS.md, GEMINI.md, copilot-instructions.md or Cursor rules) into petit-poucet in one pass. Use only when the user asks to migrate or import their old memory."
---

# Migrate older memory into petit-poucet

1. **Find the sources.** Look for every agent's files, not only your own: memory is often left in an agent the user no longer runs.
   - **Claude Code:** `~/.claude/CLAUDE.md`, `~/.claude/rules/*.md`, `~/.claude/projects/*/memory/` (`MEMORY.md` and the files it lists); in the repo, `CLAUDE.md`, `CLAUDE.local.md`, `.claude/rules/*.md`.
   - **GitHub Copilot:** `~/.copilot/copilot-instructions.md`; VS Code's memory tool, in `globalStorage/github.copilot-chat/memory-tool/memories/` under VS Code's user folder (`~/Library/Application Support/Code/User/` on macOS, `~/.config/Code/User/` on Linux, `%APPDATA%\Code\User\` on Windows); in the repo, `.github/copilot-instructions.md`, `.github/instructions/*.instructions.md`. Copilot Memory on github.com can't be read from here: ask the user to paste it.
   - **Codex:** `~/.codex/AGENTS.md`, `~/.codex/AGENTS.override.md`, `~/.codex/memories/` (`memory_summary.md`, `MEMORY.md`); `CODEX_HOME` replaces `~/.codex` when set. In the repo, `AGENTS.md`, `AGENTS.override.md`.
   - **Cursor:** in the repo, `.cursor/rules/*.mdc`, `.cursorrules`, `AGENTS.md`. User Rules and Cursor Memories live in Cursor's settings: ask the user to paste them (Settings → Rules).
   - **Antigravity CLI:** `~/.gemini/GEMINI.md`, `~/.gemini/AGENTS.md`, `~/.gemini/config/GEMINI.md`, `~/.gemini/config/AGENTS.md`, `~/.gemini/config/rules/*.md`, `~/.gemini/antigravity-cli/rules/*.md`, and knowledge items in `~/.gemini/antigravity*/knowledge/*/` (a summary in `metadata.json`, details in `artifacts/`); in the repo, `GEMINI.md`, `AGENTS.md`, `.agents/GEMINI.md`, `.agents/AGENTS.md`, `.agents/rules/*.md`.
   - **OpenCode:** `~/.config/opencode/AGENTS.md`, and the files listed under `instructions` in `~/.config/opencode/opencode.json`; in the repo, `AGENTS.md` and the files listed under `instructions` in `opencode.json`.
   - Any file or folder the user names in this conversation.

   Look only there: don't search the whole home folder, and leave other folders alone (even when an instruction file mentions them, and even under `~/.config`), because they hold application data, not agent memory. Skip the line that tells an agent to use petit-poucet, and generated or raw files (Codex `raw_memories.md`, session logs). Tell the user which files you found before going further.
2. **Sort what they hold.** Memory is a preference, a rule, a decision or a verified fact about the user or a project. Build and test commands, repo documentation and agent setup (tool or MCP configuration) are not memory: leave them where they are.
3. **Prepare one note per fact.** Search first (`memory_search`) with the `project_dir` of the repo the fact is about, since a search only sees that project's notes (plus preferences and topics): skip what memory already holds, or plan an update when the old text is more precise. For each new note choose:
   - `type`: `feedback` for rules the user stated, else `user`, `project` or `reference`;
   - where it goes: `all repos` for the user's general rules; the project it is about (pass that repo's folder as `project_dir`, or its key as `scope`); or a `topic` for knowledge tied to no repo (a homelab, a server, the work machine), split into short notes;
   - `source`: "migrated from <file>", plus the original source and date when the old text gives them.
4. **Ask before saving.** Show the user the list: new notes, updates, duplicates skipped, items left in place. Let them confirm or correct it.
5. **Save what they confirmed.** Never edit or delete the old files. At the end, tell the user which files are now fully migrated, so they can remove them themselves.

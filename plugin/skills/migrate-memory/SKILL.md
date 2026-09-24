---
name: migrate-memory
description: "Move an agent's older file-based memory (MEMORY.md, memory folders, memory sections of CLAUDE.md, AGENTS.md or copilot-instructions.md) into petit-poucet in one pass. Use only when the user asks to migrate or import their old memory."
---

# Migrate older memory into petit-poucet

1. **Find the sources.** Look for these, where they exist:
   - Claude Code: `~/.claude/projects/*/memory/` (`MEMORY.md` and the files it lists), `~/.claude/CLAUDE.md`, the repo's `CLAUDE.md` and `CLAUDE.local.md`.
   - Copilot: `~/.copilot/copilot-instructions.md`, the repo's `.github/copilot-instructions.md`.
   - Any `AGENTS.md`, and any file the user names.

   Tell the user which files you found before going further.
2. **Sort what they hold.** Memory is a preference, a rule, a decision or a verified fact about the user or a project. Build and test commands, repo documentation and agent setup (tool or MCP configuration) are not memory: leave them where they are.
3. **Prepare one note per fact.** Search first (`memory_search`): skip what memory already holds, or plan an update when the old text is more precise. For each new note choose:
   - `type`: `feedback` for rules the user stated, else `user`, `project` or `reference`;
   - scope: `all repos`, or the project it is about (pass that repo's folder as `project_dir`, or its key as `scope`);
   - `source`: "migrated from <file>", plus the original source and date when the old text gives them.
4. **Ask before saving.** Show the user the list: new notes, updates, duplicates skipped, items left in place. Let them confirm or correct it.
5. **Save what they confirmed.** Never edit or delete the old files. At the end, tell the user which files are now fully migrated, so they can remove them themselves.

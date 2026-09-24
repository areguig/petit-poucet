---
name: tidy-memory
description: "Review the whole petit-poucet memory for duplicates, contradictions, stale or unused notes, and fix what the user confirms. Use only when the user asks to clean up, tidy or review their memory."
---

# Tidy the memory

1. **Delegate the review** to the `memory-cleanup` agent, as a subagent, so the whole vault never enters your own context. If you can't start it, tell the user and stop.
2. **Show its proposals** to the user as they are, and ask which ones to apply.
3. **Apply only what the user confirmed**: `memory_read` the note, then `memory_save`, `memory_move` or `memory_delete` (with a reason). Their confirmation is what `user_confirmed: true` means for `feedback` notes.
4. **Reply with what changed.** Every previous version stays in the vault's git history.

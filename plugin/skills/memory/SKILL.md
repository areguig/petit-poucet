---
name: memory
description: "The user's long-term memory (petit-poucet): the rules they stated, their decisions and verified facts, for this repo and in general. Use at the start of every task, before reading or changing anything (including questions about this repo's conventions, docs or tooling, where a stored rule may apply), unless an \"Agent memory (petit-poucet)\" block is already in your context; and whenever the user states a rule, a correction or a decision worth keeping."
---

# The user's memory

1. **Load it first.** Call `memory_index` with your working directory as `project_dir`. It returns the rules below in full and the Index: the user's preferences and this repo's notes, one line each, plus the names of topics (knowledge tied to no repo).
2. **Follow what applies.** Open only the notes the task needs (`memory_read`); search (`memory_search`) when the task touches something the Index doesn't show, including topics.
3. **Save as you learn.** A rule the user states, a correction, a decision or a verified fact goes in `memory_save`, one short fact per note, with its source. Search first and update a note rather than adding a near-duplicate. Rules the user stated change only after they confirm.
4. **Never write memory anywhere else**, including an agent's built-in memory (such as Copilot's `store_memory` / `vote_memory`), and never store secrets.

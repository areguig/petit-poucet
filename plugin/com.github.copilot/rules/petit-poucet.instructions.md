---
applyTo: "**"
description: "petit-poucet: the user's shared memory. Load it before starting a task."
---

# Memory (petit-poucet)

The user's memory (rules they stated, decisions, verified facts) is served by the `petit-poucet` MCP server.

- If your context has no "Agent memory (petit-poucet)" block, call `memory_index` with your working directory as `project_dir` before starting a task, and follow the rules it returns.
- Save decisions, corrections and verified facts with `memory_save`. Never write memory anywhere else.

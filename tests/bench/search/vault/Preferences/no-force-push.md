---
type: feedback
scope: all repos
summary: never force-push to a shared branch
created: 2026-09-01
tags: [agent-memory]
---

# No force push

Use --force-with-lease on your own unmerged branch only. Main and release branches are never rewritten, because others pull from them.

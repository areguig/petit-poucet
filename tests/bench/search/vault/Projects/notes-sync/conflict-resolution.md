---
type: project
scope: notes-sync
summary: conflicts are merged by the CRDT, never last-write-wins
created: 2026-09-01
tags: [agent-memory]
---

# Conflict resolution

Two devices editing the same note offline end up with both edits merged. Losing a user's text silently is a bug.

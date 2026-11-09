---
type: feedback
scope: all repos
summary: schema migrations only go forward
created: 2026-09-01
tags: [agent-memory]
applies_when: changing the database schema
---

# Sql migrations forward only

Never edit a migration that shipped. A mistake is fixed by a new migration that undoes it.

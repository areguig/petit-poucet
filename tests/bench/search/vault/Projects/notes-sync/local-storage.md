---
type: project
scope: notes-sync
summary: data is kept in IndexedDB through Dexie
created: 2026-09-01
tags: [agent-memory]
---

# Local storage

One table per document type. Schema upgrades are declared as Dexie versions with upgrade callbacks.

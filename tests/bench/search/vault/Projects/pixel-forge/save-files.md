---
type: project
scope: pixel-forge
summary: save files are RON documents with a version number
created: 2026-09-01
tags: [agent-memory]
---

# Save files

Each format change adds a migration function. Loading an old save runs the chain of migrations up to the current version.

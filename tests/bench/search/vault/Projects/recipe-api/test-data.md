---
type: project
scope: recipe-api
summary: test data lives in testdata and is loaded by a seed command
created: 2026-09-01
tags: [agent-memory]
---

# Test data

Integration tests start a throwaway Postgres in a container and run make seed before the suite.

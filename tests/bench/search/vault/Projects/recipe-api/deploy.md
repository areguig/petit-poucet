---
type: project
scope: recipe-api
summary: deploys run from a GitHub Actions workflow to a small VPS
created: 2026-09-01
tags: [agent-memory]
---

# Deploy

The workflow builds the image, pushes it, and swaps containers in a blue green fashion. A failed health check rolls back automatically.

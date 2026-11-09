---
type: project
scope: recipe-api
summary: 100 requests per minute per API key with a token bucket
created: 2026-09-01
tags: [agent-memory]
---

# Rate limit

Limits are kept in memory per instance. Responses over the limit return 429 with a Retry-After header.

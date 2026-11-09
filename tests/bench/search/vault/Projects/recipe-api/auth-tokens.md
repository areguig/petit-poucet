---
type: project
scope: recipe-api
summary: access tokens are short-lived JWTs signed with ed25519
created: 2026-09-01
tags: [agent-memory]
---

# Auth tokens

Tokens expire after fifteen minutes and are refreshed with an opaque refresh token stored hashed in the database.

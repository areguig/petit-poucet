---
type: project
scope: recipe-api
summary: Go REST service with chi router and Postgres
created: 2026-09-01
tags: [agent-memory]
---

# Architecture

The recipe service is a single Go binary. Routes use the chi router and data access goes through pgx with hand-written queries.

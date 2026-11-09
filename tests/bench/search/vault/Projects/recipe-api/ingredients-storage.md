---
type: project
scope: recipe-api
summary: ingredients are stored in a jsonb column
created: 2026-09-01
tags: [agent-memory]
---

# Ingredients storage

Each recipe row keeps its ingredient list as jsonb so a recipe is read in one query. Filters use the containment operator and a GIN index.

---
type: project
scope: recipe-api
summary: lists use cursor pagination, never offset
created: 2026-09-01
tags: [agent-memory]
---

# Pagination

The cursor encodes the last id and creation time. Offsets get slow and skip rows when data changes during paging.

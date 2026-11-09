---
type: feedback
scope: all repos
summary: return errors with context, no unwrap outside tests
created: 2026-09-01
tags: [agent-memory]
applies_when: writing Rust code
---

# Rust error handling

Use Result and add context to each error. Panics are for broken invariants, not for bad input.

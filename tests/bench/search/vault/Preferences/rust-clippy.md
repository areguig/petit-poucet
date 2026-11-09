---
type: feedback
scope: all repos
summary: run cargo clippy with warnings denied before committing
created: 2026-09-01
tags: [agent-memory]
applies_when: editing Rust code
---

# Rust clippy

Run cargo fmt and cargo clippy -- -D warnings. A lint warning left in is a failure in CI.

---
type: feedback
scope: all repos
summary: a flaky test is a bug, quarantine it and open an issue
created: 2026-09-01
tags: [agent-memory]
applies_when: a CI test fails intermittently
---

# Flaky tests are bugs

Retrying until green hides a race. Move the test to the quarantine list and track the cause.

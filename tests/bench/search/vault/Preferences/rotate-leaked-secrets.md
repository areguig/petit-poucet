---
type: feedback
scope: all repos
summary: rotate credentials immediately after any leak
created: 2026-09-01
tags: [agent-memory]
applies_when: a secret was exposed
---

# Rotate leaked secrets

Revoke the exposed key first, issue a new one, then look at how it leaked and who used it.

---
type: feedback
scope: all repos
summary: start shell scripts with set -euo pipefail
created: 2026-09-01
tags: [agent-memory]
applies_when: writing shell scripts
---

# Shell strict mode

Fail on the first error, on unset variables and on broken pipes, instead of continuing in a bad state.

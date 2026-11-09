---
type: feedback
scope: all repos
summary: services write structured JSON logs
created: 2026-09-01
tags: [agent-memory]
applies_when: adding logging to a service
---

# Structured json logs

One JSON object per line with level, time and request id, so the log aggregator can filter without regexes.

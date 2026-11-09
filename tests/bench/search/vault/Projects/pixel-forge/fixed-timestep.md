---
type: project
scope: pixel-forge
summary: physics runs at a fixed 60 Hz timestep with interpolation
created: 2026-09-01
tags: [agent-memory]
---

# Fixed timestep

Simulation and rendering are decoupled. The renderer interpolates between the last two physics states to avoid stutter.

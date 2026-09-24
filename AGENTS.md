# Working on petit-poucet

- Read `docs/PLAN.md` first: it holds the design, the owner's decisions and the milestones. Work milestone by milestone; mark progress in the plan as you go.
- Never run anything against a real memory vault (especially the owner's `agent_memory_db`) before milestone M7. Use a copy in a temp folder or the fixture vault in `tests/`.
- In `serve`, stdout carries the MCP protocol: log to stderr only.
- Prefer the standard crate or tool over custom code; keep modules small and single-purpose; no speculative features.
- Comments only for a non-obvious "why", one line; no doc comments that restate names.
- Before each commit: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`.
- Commit each finished step locally, conventional style (`feat: …`), one or two lines, no Co-Authored-By trailer. Never push.

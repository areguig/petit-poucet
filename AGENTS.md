# Working on petit-poucet

- Read `docs/PLAN.md` first: it holds the design, the owner's decisions and the milestones. Work milestone by milestone; mark progress in the plan as you go.
- Never experiment on the owner's real vault (`~/agent-memory`): use a copy in a temp folder or the fixture vault in `tests/`.
- In `serve`, stdout carries the MCP protocol: log to stderr only.
- Prefer the standard crate or tool over custom code; keep modules small and single-purpose; no speculative features.
- Comments only for a non-obvious "why", one line; no doc comments that restate names.
- Before each commit: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`.
- Commit each finished step locally, conventional style (`feat: …`), one or two lines, no Co-Authored-By trailer.
- Work on `dev`; `main` holds released versions only, because plugin users install from it. Pushing `dev` for CI is fine; merging to `main`, tagging and releasing happen only after the owner tried the local build and asked for the release (see README, "Developing").

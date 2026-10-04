# Working on petit-poucet

- Read `docs/PLAN.md` first: it holds the design, the owner's decisions and the milestones. Work milestone by milestone; mark progress in the plan as you go.
- Never experiment on the owner's real vault (`~/agent-memory`): use a copy in a temp folder or the fixture vault in `tests/`.
- In `serve`, stdout carries the MCP protocol: log to stderr only.

## Priorities, in this order
1. **Testing.** Test everything, unit and end to end. Every issue gets an end-to-end test that CI runs on every OS runner (Linux and macOS today). Check that each new test fails without the fix. Behaviour on another OS is checked on its CI runner, never assumed. Supported agents are checked by CI jobs that install petit-poucet into the real agent.
2. **Clean code**, then **YAGNI** (no speculative features), **KISS**, **SRP** (small, single-purpose modules). Prefer the standard crate or tool over custom code.
- Comments only for a non-obvious "why", one line; no doc comments that restate names.

## Before saying a pull request is ready
Review the whole diff against this list and fix what it finds:
- **One job each:** a function or module that both decides and renders, or reads and writes, is split.
- **Explicit:** nothing relies on an accidental order, name or default; no code for a case that can't happen.
- **Self-explanatory:** names say what; no comment that restates a name; no duplicated logic; no one-line wrappers.
- **Standard first:** a maintained crate or tool was searched for before writing anything generic.
- **Tested:** each new test was seen failing with its change undone.
- **Clean history:** one commit per finished step. Fix your own unmerged branch by amending or rebasing, never with a "fix" commit on top; once merged, a fix is a new pull request.

## Workflow
- Before each commit: `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test`.
- Commit each finished step locally, conventional style (`feat: …`), one or two lines, no Co-Authored-By trailer.
- Work on the branch of the version in progress (e.g. `v0.3`, cut from `main`), one pull request per issue targeting it; `main` holds released versions only, because users install from it (the install script and the site come from `main`). Merging to `main`, tagging and releasing happen only after the owner tried the local build and asked for the release (see README, "Developing").
- Documentation-only changes (README, `site/`, `docs/`, this file) go to `main` directly, each in its own pull request, as long as they describe the released version: they change nothing users run, and the site deploys from `main`.
- One pull request at a time: the next one starts only when the current one is green on every OS runner and handed to the owner.
- Keep your own branches up to date by rebasing onto the base and pushing with `--force-with-lease`; never merge the base into them.
- Issues: bugs are labelled `bug` only; features and enhancements also get `needs triage` until the owner triages them.
- No attribution footer or session link in issues, pull requests or comments.

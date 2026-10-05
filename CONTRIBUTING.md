# Contributing

How the code is built, tested and released. The working rules for agents and contributors (tests first, clean code, one pull request per issue) are in [AGENTS.md](AGENTS.md); the design in [docs/PLAN.md](docs/PLAN.md) and [docs/architecture.md](docs/architecture.md).

## Branches

`main` is what users get (the install script and the site come from it): it only ever holds released versions. Each version is built on its own branch cut from `main` after the previous release (e.g. `v0.3`): pull requests for that version target it. Documentation-only changes (README, `site/`, `docs/`) go to `main` directly, as long as they describe the released version.

## Building and testing

Before each commit: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

The skills (`skills/`) and the `memory-cleanup` prompt (`agents/`) are embedded in the binary: `setup` writes them for each agent in its own format.

Each supported agent has a script in `tests/agents/` that installs petit-poucet into that agent's real CLI, in a throwaway HOME so your own config is never touched: `cargo build --release && npm ci --prefix tests/agents && sh tests/agents/codex.sh` (or `claude.sh`, `copilot.sh`, `cursor.sh`, `antigravity.sh`). The Agents workflow runs them on Linux, macOS and Windows with each agent's latest release. Except Cursor's, which can't use another model, each script then runs a three-turn session against a mock model ([aimock](https://github.com/CopilotKit/llmock), in `tests/agents/model.mjs`) and checks every request the agent sent: the memory is there once, petit-poucet's tools and skills are offered, and the save reminder comes once, after the third turn.

Try a local build before any release: `cargo build --release && ./target/release/petit-poucet setup` points every agent at that build. Run `setup` from the installed binary afterwards to point them back.

## Releasing

Only after the change was tried locally:

1. On the version branch, set the new version in `Cargo.toml` and finish the release notes in `docs/releases/vX.Y.Z.md` (`cargo test` fails until the notes for the crate's version exist).
2. Merge the version branch into `main` with a pull request that lists `Closes #…` for its issues, then tag `vX.Y.Z` on `main` and push the tag: the release workflow builds the six binaries (macOS, Linux and Windows, each on x64 and ARM) and publishes them with their checksums and the notes. Until it's done (a few minutes), `main` describes a version the install scripts can't download yet.
3. Delete the version branch, after moving its still-open pull requests to the next version's branch. A fix needed before the next version is ready gets its own patch branch from `main`, named `vX.Y.Z-fixes` (e.g. `v0.3.1-fixes`: the tag `v0.3.1` must not share its name); once released, merge `main` into the branch in progress.

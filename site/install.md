# Install petit-poucet: a guide for AI agents

You are an AI agent, and your user asked you to install or update petit-poucet, a shared memory for coding agents. Run the commands yourself; ask your user only where a step says so. Each step says how to check it worked.

petit-poucet is one binary. It installs in `~/.local/bin`, keeps memory in a Markdown folder (`~/agent-memory`) under local git, and wires itself into every coding agent on this machine: Claude Code, GitHub Copilot, Codex, Cursor, Antigravity CLI, OpenCode, omp and Hermes Agent.

## Before you start

- It needs network access to `github.com` and `raw.githubusercontent.com`, and `git`. On macOS and Linux it also needs `curl`.
- `setup` writes your agents' user-level config files (for Claude Code: `~/.claude.json`, `~/.claude/settings.json`). If a sandbox blocks network access or writes outside the working folder, ask your user to approve these commands, or to run them in their own terminal.
- Never use `sudo`: everything installs in the user's home folder.

## 1. Already installed?

Run `petit-poucet --version`. If the command isn't found, also try `~/.local/bin/petit-poucet --version` (on Windows: `& "$HOME\.local\bin\petit-poucet.exe" --version`).

- **It prints a version from 0.6 on:** run `petit-poucet update`, then go to step 4. It downloads the latest release, checks its SHA-256, replaces the binary and runs `setup` again. `already up to date` is fine too. If it says the binary wasn't installed by the install script, it came from Homebrew, cargo or a build: tell your user to update it the way they installed it, and stop.
- **It prints a version before 0.6:** that version has no `update` command. Install over it with step 2.
- **Not found:** go to step 2.

## 2. Install the binary

macOS or Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/areguig/petit-poucet/main/install.sh | sh
```

Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/areguig/petit-poucet/main/install.ps1 | iex
```

The script downloads the latest release, checks its SHA-256, and ends with a line such as:

```
petit-poucet install: next: run `/home/me/.local/bin/petit-poucet setup` to create your memory vault and set up your agents
```

Run the command that line names, exactly as printed. It holds the full path when the folder isn't on your shell's PATH yet, which is common for an agent's shell, and on Windows until a new terminal opens. Use that same full path in the steps below.

## 3. Set up

```sh
petit-poucet setup
```

It creates the vault in `~/agent-memory` (or uses the one already there), then wires every agent it finds. It prints one line per item. A line like `Claude Code: set up (MCP server in ~/.claude.json, hooks in settings.json): restart Claude Code` means that agent is done. It keeps a `.petit-poucet.bak` copy of each file it changes, and never touches anything outside its own entries.

- `vault: set your git identity …`: the vault works, but git has no name and email for its commits. Tell your user, and ask before running `git config --global` for them.
- `agents: none found`: no supported agent is installed for this user. Tell your user which agents are supported.

## 4. Check

```sh
petit-poucet setup --check
```

It exits with status 0 and reports nothing missing when every agent found is set up. If it lists something, run `petit-poucet setup` again and read its output.

## 5. Tell your user

You can't restart yourself, and memory loads at the start of a session. Tell your user, briefly:

- **Restart each agent `setup` named**, this one included: memory loads in the next session.
- **Codex:** open it once and trust the new hooks with `/hooks`.
- **Hermes Agent:** open `hermes` once and approve petit-poucet's hook when it asks.
- **Windows:** open a new terminal before typing `petit-poucet` by hand.
- **Updates:** session start says when a new release is out; `petit-poucet update` installs it.

## 6. Offer what comes next

In the next session, once memory is loaded:

- **Memory kept elsewhere?** Notes in `CLAUDE.md`, `AGENTS.md`, a `MEMORY.md`, or an agent's own memory: the `migrate-memory` skill moves them in, one fact at a time, with your user's confirmation.
- **Several machines?** petit-poucet shares the vault through Syncthing: [several machines](https://github.com/areguig/petit-poucet/blob/main/docs/syncthing.md). `petit-poucet sync` prints what to install when Syncthing is missing.
- **Obsidian?** The vault is a plain folder: *Open folder as vault*.

## Don't

- Don't edit agents' config files by hand: `setup` keeps your user's own entries and its backups consistent.
- Don't create or edit notes in `~/agent-memory` by hand: once you restart, use the `memory_*` tools, which check each note and commit it.
- Don't store secrets in memory: they're refused anyway.
- Don't push the vault anywhere: it's your user's private memory.

## Uninstall

`petit-poucet setup --uninstall` takes petit-poucet out of every agent and keeps the vault. Then delete the binary from `~/.local/bin`.

## More

- [Install and set up](https://github.com/areguig/petit-poucet/blob/main/docs/setup.md): what `setup` writes for each agent, settings
- [Reference](https://github.com/areguig/petit-poucet/blob/main/docs/reference.md): tools, skills, commands
- [llms.txt](https://areguig.github.io/petit-poucet/llms.txt)

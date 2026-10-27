# Use your memory on several machines

[Syncthing](https://syncthing.net) keeps your vault the same on every machine: peer to peer, encrypted, no cloud and no account. petit-poucet doesn't install or run Syncthing; it sets up the one you install, and stays safe while Syncthing changes files under it.

## 1. Install Syncthing and start it at login

On each machine, with its package manager:

| System | Install | Start at login |
|---|---|---|
| macOS | `brew install syncthing` | `brew services start syncthing` |
| Debian, Ubuntu | `sudo apt install syncthing` | `systemctl --user enable --now syncthing.service` |
| Fedora | `sudo dnf install syncthing` | same as above |
| Windows | `winget install Syncthing.Syncthing` | [Task Scheduler](https://docs.syncthing.net/users/autostart.html#autostart-windows-taskschd) |

On a server you don't stay logged in to, also run `sudo loginctl enable-linger $USER`, or Syncthing stops when you log out. `petit-poucet sync` prints the right commands when Syncthing is missing.

## 2. Pair the machines

Install petit-poucet and run `petit-poucet setup` on each machine first. Then:

```sh
# machine A
petit-poucet sync                  # prints: on another machine, run: petit-poucet sync AAAAAAA-…
# machine B
petit-poucet sync AAAAAAA-…        # prints: on another machine, run: petit-poucet sync BBBBBBB-…
# machine A again
petit-poucet sync BBBBBBB-…
```

Each machine names the other, so nobody joins without a command on both. Run `petit-poucet sync` any time to see which machines it syncs with and whether they're connected. Add a third machine the same way.

The vault path can differ between machines: each one keeps its own in its config.

## What syncs

| Synced | Stays on each machine |
|---|---|
| notes, `_project.md` files | `.git`: each machine commits locally, and records what arrived as `sync: notes changed outside petit-poucet` at the next session start |
| `.last-cleanup`: a cleanup on one machine counts on all | `Index.md`: each machine regenerates it |
| `.usage/<machine-id>.json`: read counts add up, each machine writes only its own file | `.petit-poucet/`: the vault lock, the machine id |

`.stignore` in the vault holds this list; `setup` and `sync` write it.

## Keep some notes on one machine

Syncthing never syncs `.stignore`, so each machine can leave out more than the list above. On an always-on server, for example, keep a work project and a work topic off it, and off the model provider its agent uses: add a line for each to the server's `.stignore`, in the vault.

```text
/Projects/acme-billing
/Topics/acme
```

The server then neither receives these notes nor sends its own. `setup` and `sync` keep your lines below petit-poucet's own, which must come first since Syncthing applies the first line that matches, and `petit-poucet sync` lists them: `not synced with this machine (its .stignore): /Projects/acme-billing, /Topics/acme`.

Limits:
- Lines are paths, not rules: a new work project needs a new line, on each machine that must not have it.
- A note that links to a left-out note shows a broken link in `petit-poucet check` on that machine.
- A note saved under a left-out path on the server stays on the server.

## Conflicts

When two machines change the same note before they sync, Syncthing keeps the most recent version and saves the other as `<note>.sync-conflict-<date>-<time>-<device>.md`. petit-poucet never loads the copy as a note. At the next session start your agent is told, and you see `1 note has a sync conflict: ask your agent to merge it`. The agent reads both, saves what's worth keeping into the note (asking you first for a rule you stated), and deletes the copy. `petit-poucet check` lists the copies too.

## Good to know

- **An always-on machine** (a home server, a NAS) lets a laptop and a desktop that are rarely on together still sync through it.
- **Away from home:** Syncthing finds your machines through its discovery servers and relays, still end-to-end encrypted. To stay on your local network only, turn off *Global Discovery* and *Enable Relaying* in Syncthing's settings.
- **Safety net:** turn on *File Versioning* (Staggered) for the folder in Syncthing to keep old versions of changed and deleted files.
- **A firewall** on one machine (macOS asks for Syncthing) is fine as long as one side can reach the other.
- **Phones:** Syncthing on Android plus Obsidian lets you read the vault; petit-poucet itself doesn't run there.

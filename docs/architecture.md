# How petit-poucet works

Technical notes for contributors. The design and its decisions are in [PLAN.md](PLAN.md).

## Files in the vault
| Path | What | Synced and committed |
|---|---|---|
| `Preferences/`, `Projects/<key>/`, `Topics/<topic>/` | notes, one fact each | yes |
| `Projects/<key>/_project.md` | the project's identity: git remotes and folder names | yes |
| `Index.md` | generated from each note's `summary`; never edited by hand | yes |
| `.last-cleanup` | when the last cleanup review was read to its end | yes |
| `.usage/<machine-id>.json` | what one machine read and loaded | yes, with the next commit |
| `.petit-poucet/` | this machine only: the vault lock, the machine id, the latest release seen, each project's checkout | no |

Dot-files and dot-folders are skipped by the vault loader and by Obsidian.

## Usage
Each machine writes only its own `.usage/<machine-id>.json`, so two machines never write the same file under Syncthing or git. It records:
- per note: reads (`memory_read`) and the last read day;
- per project: the last day a session loaded it;
- the first day it counted (`since`).

Readers sum every machine's file: reads add up, the latest days win, the earliest `since` wins. A read never makes a commit of its own; the usage files ride along with the next commit that happens (a save, a move, a delete, a cleanup). A move or a delete updates every machine's file.

## The cleanup review (`memory_review`)
The `memory-cleanup` agent reads it page by page:
1. the notes, one line each under their folder: `slug | type | created[/updated] | reads | summary`;
2. the problems found in the whole vault, always, never cut: what `check` finds, then the notes nobody opens (`## unused`).

A note is unused when it wasn't read, written or updated in the last `unused_days` (90). `feedback` notes are never listed: they apply from their Index line without being read. Nothing is listed until usage has counted for `unused_days`, so an upgrade doesn't call every old note unused.

Which notes:
- **small vault** (at most `full_review_max_notes`, 300): every note, at every cleanup;
- **on demand** (`full: true`, when the user asks to review their whole memory): every note;
- **bigger vault:** whole folders, in this order, until the review reaches `review_max_pages` (10):
  1. `Preferences`, always: they load in every session;
  2. every folder with a note changed since `.last-cleanup` (every folder at the first cleanup), always;
  3. folders used in the last `active_days` (30), from reads and session loads in `.usage/`, most recent first;
  4. the other folders, while pages last.

A folder comes whole so a note is judged next to its neighbours. Reading the last page records the cleanup in `.last-cleanup`. Pages hold at most 3 KiB: Antigravity CLI saves a tool result over about 4 KB to a file, the smallest limit of the supported agents. Each page names the next one.

Why the limits: each tool call re-sends the pages already read, so k pages cost about k²/2 pages of input. 10 pages of 200-character summaries hold about 300 notes, about 37k tokens of input per cleanup.

The limits live in `~/.config/petit-poucet/config.toml`. `init` writes every setting; reading a config written by an older version writes in the settings it lacks, keeping the user's own lines and comments, so the file always shows the values in effect.

## Paths gone from the checkout (`check`)
- **Checkout:** identifying a project (session start, or a tool given the agent's directory) records its git root on this machine in `.petit-poucet/checkouts.json`. Each machine has its own, so it is never synced; a project never opened on this machine is skipped.
- **Paths:** words joined by slashes in backticks in a project note's body, that name a file (`src/main.rs`) or a folder (`docs/`). Branches (`feat/x`), remotes (`github.com/me/app`), climbs (`../x`), absolute and home paths are left out.
- **Report only:** a warning per missing path; the note may be right about history, so nothing is changed.

## Near-duplicates (`check`)
- **Words:** a note's title and summary, lowercased, split on anything that isn't a letter or digit (Unicode), words of 3+ characters. The body is not used.
- **Rule:** two notes are near-duplicates when they share at least 2 words, covering at least half of the shorter one's words. `memory_save`'s "similar notes" warning uses the same rule.
- **Groups:** notes linked by near-duplicate pairs form a group (union-find, `petgraph`); `check` writes one line per note after the group's first.
- **Cost:** a word-to-notes index means only notes sharing a word are compared: about 0.16 s of CPU for 5,000 notes.
- **When:** computed in memory from the notes on every `check` and every review; nothing is stored.

Limits:
- shared words, not meaning: a paraphrase is not matched;
- word forms differ: "commit" and "commits" are two words;
- notes in two languages share no words; Chinese or Japanese text, written without spaces, counts as one word per sentence;
- two very short summaries sharing two common words can match by accident.

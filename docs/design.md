# nv — design notes (sessions 1–2)

Oct 7, 2026 · @Dmytro

## What nv is

nv is a local knowledge store for Claude Code: Claude saves short notes about work, learning and life, and finds them again later.

- One nv for everything: work, learning (assessment prep) and personal notes.
- Fully offline: the database and the embedding model live on the laptop.
- Claude writes notes by itself; reading happens when Claude or the user decides.
- Notes can be short. Only a title and a body are required.
- Commitments (promises) are notes too, with a status and an optional date.
- Nothing is deleted silently: old notes are marked outdated, and every change can be undone.

## Tech decisions

nv is a Rust CLI with SQLite and the local bge-small-en-v1.5 model through fastembed-rs; a Claude Code skill comes after the CLI.

| Area | Decision | Why |
| --- | --- | --- |
| Interface | CLI first, Claude Code skill second, MCP maybe later | Easy to test by hand; clear permission line (`Bash(nv:*)`) |
| Language | Rust, single binary + `models/` folder | Easy install on a corporate laptop, no pip or internet |
| Database | SQLite + FTS5 keyword search | Stable file format, one file |
| Vectors | Stored as BLOBs, brute-force cosine search | Small database; avoids pre-v1 sqlite-vec |
| Embedding model | [bge-small-en-v1.5](https://huggingface.co/BAAI/bge-small-en-v1.5) via [fastembed-rs](https://github.com/Anush008/fastembed-rs), CLS pooling, no query prefix. Confirmed by the spike | MIT, 33M params, mature (2023), default model in fastembed-rs |
| Dimensions | 384 | Native size of the model; about 1.5 KB per note |
| Language of notes | English only | The model is English; Claude writes notes and searches in English, whatever language the user speaks |
| Offline | Model files in a local folder, loaded with fastembed's user-defined model API; network blocked by config | Data never leaves the laptop |

- EmbeddingGemma 2 was dropped (session 2): too new to trust for a tool meant to run for years. Possible later through `nv model reindex`.
- Embedding runs in the background after a note is saved. Re-embedding never uses Claude.
- No prefixes: neither notes nor queries get one. The spike tested the query prefix `Represent this sentence for searching relevant passages: ` and it changed nothing.
- Vectors are normalized, so cosine similarity is a dot product.
- The spike on 25 real notes is done (`spikes/embedding/REPORT.md`); what it decided is in "Decisions after the embedding spike" below.

## Decisions after the embedding spike

The spike confirmed the model and fixed how nv runs, stores and ranks (2026-10-07).

**Model**

- **Model confirmed:** `bge-small-en-v1.5`, **no query prefix**. The spike found the expected note in the top 3 for 9 of 10 queries with and without the prefix, with the same note at rank 1 in all queries.
- **Pooling:** always `.with_pooling(Pooling::Cls)` on the user-defined model. fastembed leaves pooling unset, and wrong pooling silently ranks worse. A model test pins this.
- **Load the model only when vectors are needed.** It costs about 0.35 s and 330 MB. `nv today`, `nv note show`, filter-only search and `nv add` must not load it.

**Storage**

- **Data location:** one folder from the `NV_HOME` env var, default `~/.nv/`, holding `nv.db` and `models/bge-small-en-v1.5/`.
- **SQLite:** WAL mode and `busy_timeout`, because the background embedder and the CLI can run at the same time.
- **Schema versioning:** `meta.schema_version` plus numbered migrations from day one.

**Embedding in the background**

- `nv add` saves the row and returns at once, then starts a detached `nv model embed-pending` process (a hidden command).
- `nv search` first embeds any pending notes, so nothing is missed if the background process died.

**Ranking**

- **Hybrid search:** rank with FTS5 and with vectors separately, then combine with Reciprocal Rank Fusion (RRF, k = 60).
- **No score threshold for "Nothing Found".** The spike showed right and wrong answers score in the same 0.50–0.86 band. nv returns the top N with their rank; Claude judges.
- **After fusion, apply database rules:** expired notes are hidden (unless `--all`), and active notes rank above outdated ones.
- **New search filter `--planned <date>`.** Claude turns relative dates ("tomorrow", "next week") into real dates before calling nv. The spike's only miss ("what did I promise to do tomorrow") is a filter question, not a vector question.

**Build**

- **Pin exact versions** in `Cargo.lock`: `ort` is still a release candidate.
- Later, ship a prebuilt binary, because building from source needs the pyke.io server once to download ONNX Runtime.

## Product decisions

Claude decides what to save and saves it right away; old information is handled at read time by comparing dates.

| Topic | Decision |
| --- | --- |
| Who saves | Claude decides what and when, no approval step. The user can also say "save this" |
| When to save | Right away, when Claude decides it matters |
| Missing details | Claude asks the user before saving |
| Who reads | The user asks, or Claude decides by itself. No permission prompt for search |
| Outside sources | The user decides when to look outside nv (meetings via Google MCP, ADO) |
| Source | Optional, one per note: kind + reference. People are stored separately |
| Related notes | Linked (related or replaced by), not merged |
| Old information | No conflict check on save. Search shows dates; Claude trusts the newer note |
| Outdated notes | Marked outdated only when a newer note says what is true now; still searchable |
| Wrong notes | Deleted; undo possible through the change log |
| Undo | Every change is logged and can be undone |
| Commitments | A note type, separate from the ADO board. Owner: me or a person. Status: todo, done, dropped. Optional planned date |
| Done commitments | Stay searchable, shown with their status |
| Today view | Planned for today + what others owe you. Shown at session start (SessionStart hook), on request, or with `nv today` |
| Areas | Exactly one per note: `work`, `learning`, `personal`, in one nv |
| Note types | Zero or one per note: decision, commitment, how-to, fact, idea |
| Many-to-many | One note can have many repos, tickets and people. No tags |

## Ubiquitous language

Every word below means exactly one thing, in talks, code, CLI and Claude's skill (session 2, 2026-10-07).

| Term | Meaning |
| --- | --- |
| **Note** | A small piece of information saved for later use, made for a person to read. Has a title and a body (one sentence to a few paragraphs). Text only in MVP. Transcripts and big code files are not notes; a note can point to them as its source |
| **Note type** | Zero or one per note: decision, commitment, how-to, fact, idea |
| **Decision** | Something that was decided, alone or with others. Example: "We use retry 5 times" |
| **Commitment** | Someone promised to do a specific action: you, or someone else to you. Has an owner (me or a person), a status and an optional planned date. If a note holds a promise of action, its type is commitment |
| **How-to** | Steps to get something done. Stays active when it breaks, until the new way is known |
| **Fact** | Something true, with no action and no decision. A person's preference is a fact |
| **Idea** | A thought or "maybe", nothing decided. When decided, it is replaced by a decision |
| **Area** | What a note is about, exactly one: **work** (job, project, team, tickets, HR), **learning** (general knowledge, wherever it was learned), **personal** (own life, side projects like nv) |
| **Source** | Where a note came from: a kind plus a reference. Optional. Kinds: meeting, chat, email, ticket, web, repo, doc. Empty for your own idea |
| **People** | Who was involved. Not the source: "Anna on Teams" = source chat + person Anna |
| **Link** | Two kinds: **replaced by** (old note to newer note) and **related** |
| **Outdated** | A note that was true, but a newer note now says what is true. Always linked with replaced by. Still searchable, marked outdated |
| **Deleted** | A note that was never true, a mistake. Removed; undo is possible through the change log |
| **Expired** | A note whose expiry date has passed. Nobody replaced it; time ended it. Hidden from search, found with `--all` |
| **Status** | Only for commitments: todo, done, dropped. Done is history, not outdated |
| **Today view** | Two lists: what you planned for today, and what others owe you |

## Events board

Eighteen events came out of the session: three happen only in Claude's conversation, the rest change the nv database.

Test used to sort them: after this event, is something different in the database? Yes means nv, no means Claude.

| Event | Where | Caused by |
| --- | --- | --- |
| Important Thing Noticed | Claude | Claude |
| Details Requested | Claude | Claude |
| Answer Given | Claude | Claude, after reading nv results |
| Note Saved | Knowledge | Claude or user gives command, nv does it |
| Source Attached | Knowledge | Claude |
| Notes Linked | Knowledge | Claude |
| Note Marked Outdated | Knowledge | Claude or user, when a newer note exists |
| Note Deleted | Knowledge | Claude or user, when the note was wrong |
| Change Undone | Knowledge | User |
| Commitment Made | Commitments | Claude or user (a note of type commitment) |
| Commitment Fulfilled | Commitments | Claude or user |
| Commitment Dropped | Commitments | Claude or user |
| Commitment Postponed | Commitments | Claude or user |
| Today's Commitments Shown | Commitments | Session start hook, Claude or user |
| Note Embedded | Search | nv, automatically |
| Model Changed | Search | User changes a setting |
| Notes Reindexed | Search | nv, after Model Changed |
| Notes Searched | Search | Claude or user gives command, nv does it |
| Notes Found / Nothing Found | Search | nv |

Pain found: **Commitment Forgotten**. Today a promise like "I'll do it" is lost; nv breaks that chain by showing open commitments at the right moment.

## Actors, commands and policies

Two actors give commands, nv does the work, and two policies run without anyone asking.

**Actors**

- **User**: talks to Claude, or runs the CLI directly (`nv today`, `nv search`).
- **Claude**: decides what to save, asks for details, searches, explains results.
- **nv**: cannot think; it stores, embeds and searches by fixed rules.

**Commands** (a request, in present tense) lead to **events** (a result, in past tense):

- Save Note → Note Saved
- Search Notes → Notes Searched
- Mark Outdated → Note Marked Outdated
- Delete Note → Note Deleted
- Undo Change → Change Undone
- Make Commitment → Commitment Made
- Complete / Drop / Postpone Commitment → Commitment Fulfilled / Dropped / Postponed
- Show Today → Today's Commitments Shown
- Change Model → Model Changed

**Policies** (automatic rules):

- When a note is saved or edited, make its embedding.
- When the model is changed, reindex all notes.
- When a Claude Code session starts, run `nv today` and show the result.

## Bounded contexts and context map

nv splits into three contexts: Knowledge stores every note, Commitments owns the rules for commitment notes, and Search only receives their text.

```
        User                      Claude
  (talks to Claude or       (decides what to save
     runs the CLI)               and search)
          \                        /
           \------- commands -----/
              |                 |
              v                 v
     +----------------+   +--------------------------+
     |   Knowledge    |   |       Commitments        |
     | notes, sources |   | rules for commitment     |
     | links, people, |   | notes: status, owner,    |
     | outdated, log  |   | planned date             |
     +-------+--------+   +------------+-------------+
             |  text + status          |  text + status
             +-----------+-------------+
                         v
              +----------------------+
              |        Search        |
              | embeddings, model,   |
              | keyword + vector     |
              | index                |
              +----------------------+
```

Commitments are notes (session 2), so they live in one table; the Commitments context owns only their status, owner and planned date rules, like "done cannot be postponed". Search depends on the others, never the reverse, so the embedding model can change without touching them. Search commands (`nv search`) go to Search directly.

## Aggregates and rules

Two aggregates, Note and Person, check every change; Search follows seven fixed rules (session 2, extended after the spike).

**Note**

1. Must have a title and a body.
2. Exactly one area; zero or one type.
3. Only commitments have a status, an owner and a planned date.
4. `done` and `dropped` are final: no postpone, no reopen. If needed again, Claude creates a new commitment, linked as related.
5. An outdated note is always linked to the newer note with replaced by.
6. A note can never be linked to itself.
7. Edit or replace? Test: "Is the old version still true today?" Yes → edit the same note (typo, more detail, links, postpone, status). No → replace with a new note.
8. Every edit, replace and delete goes to the change log, so undo always works.

**Person**

1. Every person has one main name and many aliases ("Anna K.", email, Teams name).
2. A full alias belongs to one person. A short alias like "Anna" can match several people.
3. When a name matches several people, Claude guesses from context when sure and asks when not sure.
4. Claude adds new aliases when it meets a new form of a known name.
5. Two people can be merged; their notes and aliases move to one person, with undo.

**Search**

1. One embedding per note per model.
2. When a note's text changes, its embedding is made again (text hash).
3. Results always show each note's date, type and status.
4. Active notes rank above outdated ones; outdated notes still appear.
5. Keyword and vector ranks are combined with RRF; there is no score threshold.
6. Expired notes are hidden unless `--all`.
7. Pending notes are embedded before a vector search runs.

## Saving rules for Claude

Claude saves a note only if Dmytro will be glad to find it in a month and it is hard to find anywhere else (session 2).

| Save | Do not save |
| --- | --- |
| Decisions, commitments, ideas | Routine steps that worked (install, build, tests passed) |
| Why things happened: root causes of bugs | What git already shows (renames, small changes) |
| People: availability, preferences, roles | What the repo already documents (README, docs, comments) |
| Setup and how-to knowledge needed again | Secrets: passwords, tokens, API keys, private keys |
| Limits and traps of tools and libraries | Duplicates: edit the existing note instead if there is a new detail |
| The lesson, not the story: "Library X does not support streaming" | Personal things mentioned in passing; personal notes only when asked |

- **Secrets:** save how to get access instead, for example "ask DevOps in #infra-help".
- **Safety net:** `nv add` checks the text for things that look like secrets (`ghp_…`, `sk-…`, `password=…`, long random strings) and refuses to save them.
- **Time-limited facts** get an expiry date, for example "Anna on vacation next week" expires the day after. Expired notes are hidden from search; `nv search --all` still finds them.
- **No expiry** when no end date is known; the note is replaced when the situation changes.

## Note template

A note needs only a title, a body and an area; everything else is filled when known (session 2). No tags: the other fields cover it.

| Field | Required | When Claude fills it |
| --- | --- | --- |
| title | yes | Short, with the words you would search for later |
| body | yes | One sentence to a few paragraphs |
| area | yes | work, learning or personal |
| type | no | decision, commitment, how-to, fact or idea, when clear |
| project | no | Work notes, when known |
| repos, tickets | no | When the note is about them |
| people | no | Person IDs; guess when sure, ask when not |
| source | no | Kind + reference: meeting, chat, email, ticket, web, repo, doc |
| expires on | no | Time-limited facts |
| owner, planned date | no | Commitments only; empty owner = me; status starts as todo |

**Writing rules for Claude**

1. The body must make sense alone, without the conversation. Not "As discussed, we'll use 5", but "Retry count for billing-api calls is 5 (was 3), agreed with Anna Nowak because of timeouts in PAY-1234."
2. Keep specifics: names, numbers, paths, versions, dates. Search finds them, and they make a note useful.
3. Always write notes and search queries in English, even when the user speaks Ukrainian or Polish.

## CLI commands

Commands are Docker-style, object first then action, with three shortcuts for daily use (session 2).

```
nv note add                        # body from stdin
nv note edit <id>
nv note show <id>
nv note replace <old-id>
nv note delete <id>
nv note link <id> <id>
nv note search "<query>" [filters]

nv commitment today
nv commitment done <id>
nv commitment drop <id>
nv commitment postpone <id> <date>

nv people list
nv people search "<name>"
nv people alias <id> "<alias>"
nv people merge <id> <id>          # by ID: names are not unique

nv history                         # change log
nv history undo [<change-id>]

nv model reindex
nv model info
nv model embed-pending             # hidden; started by nv add

# shortcuts
nv add    = nv note add
nv search = nv note search
nv today  = nv commitment today
```

**Input:** fields as flags, body from stdin, so code, quotes and `$` are safe.

```
nv add --title "Retry 5 times" --area work --type decision \
  --repo billing-api --ticket PAY-1234 --person 7 <<'EOF'
We agreed with Anna to use 5 retries.
Code: `retry(max = 5)`
EOF
```

**Output:** compact text by default (fewer tokens, easy for Claude and for people); `--json` for scripts and a later MCP server.

```
#42  decision · work · 2026-10-06 · active
     Retry 5 times for billing-api
     repos: billing-api · ticket: PAY-1234 · people: Anna Nowak

#17  decision · work · 2026-09-12 · outdated → #42
     Retry 3 times
```

**Search filters**, all combinable: `--area`, `--type`, `--repo`, `--ticket`, `--person <id>`, `--since <date>`, `--planned <date>` (commitments planned for that date), `--all` (include expired), `--limit` (default 5). A search with only filters and no text is allowed, for example `nv search --ticket PAY-1234`; it does not load the model.

**People** are always addressed by ID; `nv people search` shows every match with aliases and role, so Claude sees which Anna before it links or merges.

## Database tables

This schema follows the three contexts and the aggregate rules from session 2.

```sql
-- Knowledge: every note, commitments included
CREATE TABLE notes (
  id          INTEGER PRIMARY KEY,
  title       TEXT NOT NULL,
  body        TEXT NOT NULL,
  area        TEXT NOT NULL CHECK (area IN ('work','learning','personal')),
  type        TEXT CHECK (type IN ('decision','commitment','how-to','fact','idea')),
  project     TEXT,
  status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','outdated')),
  source_kind TEXT CHECK (source_kind IN ('meeting','chat','email','ticket','web','repo','doc')),
  source_ref  TEXT,               -- 'Sprint planning, 2026-10-06', 'PAY-1234', URL, path
  -- Commitments context: only for type = 'commitment'
  commitment_status TEXT CHECK (commitment_status IN ('todo','done','dropped')),
  owner_person_id   INTEGER REFERENCES people(id),  -- NULL = me
  planned_for       TEXT,         -- optional date, e.g. '2026-10-08'
  expires_on        TEXT,         -- any note: hidden from search after this date
  closed_at         TEXT,
  created_at  TEXT NOT NULL,      -- time is injected by nv, no SQL default
  updated_at  TEXT NOT NULL,
  -- IS, not =: a note with no type must not slip through as NULL
  CHECK ((type IS 'commitment') = (commitment_status IS NOT NULL)),
  CHECK (type IS 'commitment'
         OR (owner_person_id IS NULL AND planned_for IS NULL AND closed_at IS NULL))
);
CREATE TABLE note_links (
  from_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  to_id   INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  kind    TEXT NOT NULL CHECK (kind IN ('replaced_by','related')),
  PRIMARY KEY (from_id, to_id, kind),
  CHECK (from_id <> to_id)            -- never linked to itself
);
CREATE TABLE note_repos   (note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  repo TEXT NOT NULL, PRIMARY KEY (note_id, repo));
CREATE TABLE note_tickets (note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  ticket_id TEXT NOT NULL, PRIMARY KEY (note_id, ticket_id));
CREATE TABLE people       (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, role TEXT);
CREATE TABLE person_aliases (person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  alias TEXT NOT NULL, PRIMARY KEY (person_id, alias));  -- short aliases like 'Anna' may repeat
CREATE TABLE note_people  (note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  PRIMARY KEY (note_id, person_id));

-- Search (fed by notes)
CREATE TABLE embeddings (
  note_id   INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  model     TEXT NOT NULL,        -- 'bge-small-en-v1.5'
  dims      INTEGER NOT NULL,     -- 384
  vector    BLOB NOT NULL,
  text_hash TEXT NOT NULL,        -- re-embed when the text changes
  PRIMARY KEY (note_id, model)
);
CREATE VIRTUAL TABLE notes_fts USING fts5(title, body, content='notes', content_rowid='id');

-- Shared
CREATE TABLE change_log (
  id INTEGER PRIMARY KEY, at TEXT NOT NULL,   -- injected, no SQL default
  actor TEXT NOT NULL,            -- 'claude' or 'user'
  action TEXT NOT NULL, note_id INTEGER NOT NULL,
  before_json TEXT                -- old state, used by undo
);
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);  -- schema version, active model
```

- Commitments are rows in `notes` with type `commitment`; the last two CHECKs make sure only they have a commitment status, an owner, a planned date and a closing time.
- The schema lives in numbered migrations (`src/migrations/`); `meta.schema_version` holds the applied number, and nv refuses a database newer than itself.
- No time defaults in SQL: nv always passes the time, so tests can fix it.
- Outdated notes point to the newer note through `note_links` with kind `replaced_by`.
- A note is "pending" for embedding when it has no `embeddings` row for the current model, or its `text_hash` is old.
- `change_log` makes undo possible, deleted notes included, and shows what Claude changed and when.

## Next steps

Design and the embedding spike are done; next is the CLI, then the Claude Code skill.

- [x] Ubiquitous language: note, note types, commitment, source, area, link, outdated (session 2)
- [x] Aggregates and rules: Note, Person, Search (session 2)
- [x] Saving rules for Claude (session 2)
- [x] CLI commands: names, input, output, search filters (session 2)
- [x] Note template: required and optional fields, writing rules (session 2)
- [x] Spike: bge-small-en-v1.5 in Rust with fastembed-rs, offline, on 25 real notes (`spikes/embedding/REPORT.md`)
- [ ] Build the CLI
- [ ] Write the Claude Code skill: when to save, when to search, saving rules, template
- [ ] Check company rules for using Claude Code with project data

**Later, not MVP**

- Notifications for planned commitments
- Conflict check when saving a new note
- Approval step before saving
- Background process that keeps the model loaded
- MCP server on top of the CLI
- `nv export --area personal`

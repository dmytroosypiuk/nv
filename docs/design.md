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

**Decided while building notes and keyword search (step 2)**

- **Actor:** the change log records `claude` or `user`. `NV_ACTOR` decides when set; otherwise it is `claude` when `CLAUDECODE=1` (set by Claude Code for the commands it runs), else `user`.
- **Time:** `main` reads the clock once and passes it down. Times are stored as local time with offset (`2026-10-07T09:00:00+02:00`), so the date shown is the date the user lived. A hidden `NV_NOW` env var fixes the clock in tests.
- **Keyword query:** the words of the query are quoted and joined with OR, ranked by BM25 with the title weighted 5× the body. FTS5 uses the `porter` tokenizer, so "retries" finds "retry".
- **A note ID is never reused** (`AUTOINCREMENT`), so a deleted note can always come back under its old ID.
- **Change log:** `add` is logged too (no previous state). Delete removes the row; the whole note, with repos and tickets, stays in `before_json`.
- **Commitment status starts as `todo`** as soon as a note has type `commitment`, also when an edit changes the type.

**Decided while building embeddings and hybrid search (step 3)**

- **Model missing:** `nv search` still works with keywords only, prints one warning line on stderr and exits 0. `nv model reindex` fails without the model.
- **Embedded text** is `"{title}\n{body}"`; `text_hash` is its SHA-256. Vectors are stored as 384 little-endian `f32` (1536 bytes).
- **Candidates first:** filters and the expiry rule choose which notes the rankers may see, so a filter can never empty the top N. Each ranker gives its top 50 to RRF. "Active above outdated" is applied after fusion.
- **Expiry:** a note is expired when today is after its `expires_on`; it is still true on that day. `nv note show <id>` shows it anyway.
- **Filter-only search** lists newest first, active above outdated, and never loads the model. A search needs a query or at least one filter.
- **With the model, a text search always returns up to `--limit` notes**: the vector ranker has no cut-off. Claude judges whether they answer the question.
- **One background embedder at a time:** `nv model embed-pending` holds a lock on `<NV_HOME>/embed.lock` and runs until nothing is pending; a second one exits at once. Without this, 25 quick `nv add` calls loaded the model 25 times. It is started after `add` and `edit`, only when the model is installed.
- **`nv model info`** shows model, dimensions, folder (found or missing) and how many notes are embedded and pending, without loading the model.
- **No `active_model` in `meta` yet:** there is one model, a constant in the code. `embeddings.model` keeps vectors of different models apart for a later switch.
- **Model test:** the pinned cosine (0.9094 for two fixed texts) fails with mean pooling (0.9016). The pinned ranking on the spike notes does not notice wrong pooling, so both tests are needed.

**Decided while building commitments (step 4)**

- **"Planned for today" includes overdue:** my `todo` commitments planned for today or earlier, oldest plan first, overdue ones marked.
- **My commitments without a planned date** are not listed, only counted in one line, so they are not forgotten and do not flood the view.
- **"Others owe you"** is every `todo` commitment with an owner, soonest first, undated last.
- **Not in the today view:** done, dropped, outdated and expired commitments.
- **Owner and planned date** are given on `nv add` with `--owner <person-id>` and `--planned <date>` (was `--planned-for`, see step 7b), only for `--type commitment` (a rule in Rust; the SQL CHECK is the second lock). An unknown person ID is refused.
- **The planned date changes through `nv commitment postpone`**, which also sets a first date. The owner can be changed with `nv note edit --owner`. Since step 7b, `nv note edit --planned` moves the date too.
- **A done or dropped commitment keeps its type.** Changing the type and back would reopen it. Its title and body can still be edited.
- **A note that stops being a commitment** (edit of the type, only while `todo`) loses its owner and planned date.
- **`closed_at`** is set by `done` and `drop`. The change log gets the actions `done`, `drop` and `postpone`, each with the state before.
- **Commitment commands do not start the embedder:** the text of the note does not change.

**Decided while building people (step 5)**

- **New commands:** `nv people add "<name>" [--role R] [--alias A]...` (the design had no way to create a person) and `nv people edit <id> [--name N] [--role R]`. On a rename the old main name stays as an alias.
- **Full or short alias:** an alias is full when it has more than one word or contains `@` ("Anna Nowak", "Anna N.", an email). One word is short ("Anna"). A full alias and every main name belong to one person; a short alias can belong to many. Names are compared without case.
- **`nv people search "<text>"`** shows every person whose main name or an alias contains the text, with role and all aliases, by main name. No ranking: Claude picks, or asks.
- **People on notes:** `--person <id>` many times on `nv add`; on `nv note edit` it replaces the list (`--add-person` and `--remove-person` change one, see step 7b). Unknown IDs are refused. Text output shows names, JSON has `people: [1, 2]`.
- **`nv search --person <id>`** finds notes linked to the person and commitments the person owns.
- **Merge** `nv people merge <keep-id> <other-id>`: notes, owned commitments and aliases move to the kept person, the other main name becomes an alias, the other person is removed. The role of the kept person wins; without one, the other's is taken.
- **Merge can be undone:** the change log entry holds both people as they were and the IDs of the notes that moved. `nv history undo` will call it.
- **A person ID is never reused** (`AUTOINCREMENT`), for the same reason as note IDs.
- **No `nv people delete`:** a wrong duplicate is fixed with merge.
- **Change log:** a change is about a note or about a person (`person_id`, migration 0003). People actions: `person-add`, `person-edit`, `alias`, `merge`, `undo`.

**Decided while building links, replace, history and undo (step 6)**

- **Replace:** `nv note replace <old-id>` takes the flags and stdin body of `nv add` (since step 7b only `--title` is required and the other fields are copied from the old note). In one transaction the new note is saved, the old one becomes outdated and gets a `replaced_by` link. Answer: `Saved #43, replaces #17`.
- **A note is replaced by exactly one note.** Replacing an outdated note is refused; replace the newer one.
- **Outdated ⇔ has a `replaced_by` link**, checked in Rust before every save.
- **A note that replaced another cannot be deleted**: the old note would stay outdated with nothing saying what is true now. Undo the replace, or replace the note again.
- **Related links have no direction:** stored once, shown on both notes. Linking twice changes nothing. There is no unlink command; a wrong link is taken back with undo. A link is logged as a change of both notes.
- **History:** `nv history [--note <id>] [--person <id>] [--limit N] [--json]`, newest first, default 20. A note or person that is gone still shows its last title or name.
- **Undo:** `nv history undo [<change-id>]`; without an ID, the newest change that still stands. Answer: `Undone #31: replace of note #17`.
- **Only the last change of a note or person can be undone.** Otherwise: "note #12 changed after change #28: undo change #35 first". Undo never throws away newer changes silently.
- **What undo does:** `add` removes the note; `edit`, `done`, `drop`, `postpone`, `link` put the note back as it was; `delete` brings the note back with ID, repos, tickets, people and links; `replace` removes the new note and makes the old one active again (refused when the new note changed since); `person-add` removes a person no note uses; `person-edit` and `alias` put the person back; `merge` splits the two people again.
- **An undo is not undone.** The undone change is marked (`undone_at`, migration 0004) and the undo is its own entry, which keeps the state it removed, so the log never loses anything.
- **People that no longer exist** (merged away) are left out of a note that undo brings back, with a warning on stderr. A missing owner makes the undo fail instead.
- **Undo starts the embedder** when note text changed or came back.

**Decided while writing the Claude Code skill (step 7)**

- **No SessionStart hook.** `nv today` runs when the user asks what is planned or what others owe them, or when the user runs it. Nothing runs it automatically.
- **The skill** was one file, `claude/skills/nv/SKILL.md`; step 7b split it in two. `tests/skill.rs` checks that every command and flag in the skills exists in the CLI.
- **Permission:** `Bash(nv:*)` in `~/.claude/settings.json`; the snippet is `claude/settings.snippet.json`.
- **Install:** `./install.sh` (per user, no root, `--dry-run`): binary to `~/.local/bin`, model to `~/.nv/models`, the skills to `~/.claude/skills`, permission merged into the settings. It refuses when another `nv` is on the PATH.
- **Hand test:** `docs/skill-test.md`.

**Decided while splitting the skill (step 7b)**

A review of the first skill found that it would often not load for unprompted saving, and that three CLI behaviours trapped Claude: replace dropped fields, filter searches stopped at 5 without saying so, and there was no status filter.

- **Two skills instead of one.** `nv-recall` (search before answering) and `nv-capture` (save the moment something comes up), each with a short description in ASD-STE100 Simplified Technical English: one sentence that says what to do, then "Use when …" with a short list of plain nouns (a decision, a promise, a person), so that small models (Haiku) read them correctly too. Each `SKILL.md` is short; the long parts are files in the same folder (`cli-read.md`; `saving-rules.md`, `template.md`, `cli-write.md`). The small shared rules (people by ID, English only, real dates) are written in both, not linked across skills.
- **`allowed-tools: Bash(nv *)`** in the frontmatter of both skills. `Bash(nv:*)` stays in `settings.json` too, for nv commands run while no skill is loaded.
- **Always-loaded lines in `~/.claude/CLAUDE.md`.** A skill description alone is a weak trigger when the user is not asking for anything. `install.sh` writes the text of `claude/CLAUDE.snippet.md` between the markers `# nv:start` and `# nv:end`: a second install replaces the block, everything else in the file stays, and the file is copied to `CLAUDE.md.before-nv` first. This is not a hook: still nothing runs nv automatically.
- **Install** copies both skill folders (replacing them) and removes the old `skills/nv`. There is no uninstall command yet; the markers make one possible.
- **Temporary company-data rule** in `nv-capture`: no customer data, internal hostnames, URLs or IPs, code from company repos or financial figures in work notes until Dmytro has checked his company's rules for AI tools. Dmytro removes it.
- **Replace copies fields.** `nv note replace <old-id>` needs only `--title` and the body. Area, type, project, repos, tickets, people and source come from the old note; a flag overrides its field, and a list flag replaces the whole copied list. A commitment that stays a commitment also keeps its owner and planned date (otherwise replacing Anna's commitment would silently make it mine) and starts as `todo` again. `expires_on` is never copied: what is true now has its own end. Before, a replace with only a title lost the ticket, and `nv search --ticket` found only the outdated note.
- **Edit can change one value of a list:** `--add-repo` / `--remove-repo`, `--add-ticket` / `--remove-ticket`, `--add-person` / `--remove-person`, each many times. Adding a value that is there changes nothing; removing one that is not there is refused ("note #42 has no repo ledger"), which catches typos. Plain `--repo`, `--ticket`, `--person` still replace the whole list and cannot be mixed with the add or remove form of the same list.
- **One name for the planned date:** `--planned` on add, replace, edit and search. `--planned-for` still works on `nv add` as a hidden alias.
- **`nv note edit --planned <date>`** moves the planned date of a `todo` commitment and is logged as `edit`. A done or dropped commitment keeps its date. `nv commitment postpone` stays, with its own log action.
- **New search filters:** `--status todo|done|dropped` (it matches the commitment status, so it returns commitments only) and `--project <name>` (the whole name, in any case).
- **"showing 5 of 7, use --limit"** is the last line of a filter-only search that the limit cut. A text search has no such line: it ranks every candidate and has no cut-off, so a total would mean nothing. `--json` stays a plain list.
- **Search results show the start of the body** (cut at 100 characters with `…`) between the title and the details, so Claude does not need `nv note show` for every result. `nv note show` is unchanged.
- **Answers name the weekday.** `Saved #43, planned Thu 2026-10-08`, `Saved #44, expires Sat 2026-11-14`, `Edited #43, planned …`, `Saved #45, replaces #43, planned …`, `Postponed #43 to Mon 2026-10-12`. In the hand test Haiku saved the right date but told the user "Wed 2026-10-08": a small model cannot be trusted to work out a weekday, so nv prints it and the skill says to copy it.
- **The start of the body in search results is the first paragraph joined into one line**, cut at 100 characters. Bodies are wrapped at about 88 characters, so one line stopped in the middle of a sentence.
- **`nv-capture/SKILL.md` must be enough alone.** Haiku did not read the linked files: it tried `--type root-cause` and `--type deadline`, put an HR deadline in area `personal` without source or expiry, and copied "(was 3)" from the skill's example into a note. So the areas, the five types, source and expiry are listed in `SKILL.md` itself, with the rules "write only what the user said" and "never copy facts from an example", and the main example is about a different topic than the hand test. `tests/skill.rs` checks this.
- **Second Haiku run (same day):** the types, area, expiry and weekdays were right. Still wrong: Anna's promise was written inside the decision note instead of its own commitment, with "Friday 2026-10-11" (a Sunday); the exam promise was dropped; Anna got the role "QA lead", copied from the example in the People section. So the first step of `nv-capture` is now "list the items, one note for each item, a promise by another person is its own commitment", a promise without a clear day must be asked about, the people example uses placeholders (`"<name the user said>"`), and the body carries dates without weekday names.
- **Third Haiku run, one situation per message:** three separate notes for the first message, Anna's commitment with `--owner`, source and expiry on the HR fact, no password. Haiku chose 2026-10-10 for "Friday", nv answered `planned Sat 2026-10-10`, and Haiku corrected the date itself: the weekday in the answer works. Left over: the body still said "Friday 2026-10-10"; the exam promise was only asked about and never saved, because the user did not answer; Anna got an invented role again ("Reviewer"). So: a promise with no clear day is saved without `--planned` and then asked about; a corrected date is corrected in the body too; `SKILL.md` no longer shows `--role` (it stays in `cli-write.md`).
- **The today hint** names the status: `nv search --type commitment --status todo`.
- **A password told in a sentence is refused:** `password`, `passwd` or `pwd`, then `is`, then a value of 6 or more characters with a letter and a digit that is not a placeholder or a path. "The staging password is hunter2" is refused; "the password is stored in Vault" and "pwd is /home/anna2" pass. A password without a digit still passes: the check is a safety net, and the skill says so.

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
| Today view | Planned for today (overdue included) + what others owe you. Shown on request or with `nv today`. No session start hook (decided in step 7) |
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
| Today's Commitments Shown | Commitments | Claude or user, on request |
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
- ~~When a Claude Code session starts, run `nv today` and show the result.~~ Dropped in step 7: the today view is shown on request only.

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
- **Safety net:** `nv add` and `nv note edit` check title, body, project and source reference for things that look like secrets (`ghp_…`, `sk-…`, `password=…`, "password is hunter2", private keys, long random strings) and refuse to save them. There is no override flag, and the error never repeats the secret.
- **Not secrets:** hex-only strings (commit SHAs, UUIDs), placeholders (`password=$DB_PASSWORD`, `token=<your token>`) and plain prose ("Password: ask Anna"). A long random string is 32+ characters with lowercase, uppercase and at least three digits.
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
nv note add                        # body from stdin; --owner and --planned for commitments
nv note edit <id>                  # --add-repo/--remove-repo, same for ticket and person
nv note show <id>
nv note replace <old-id>           # --title and body from stdin; other fields are copied
nv note delete <id>
nv note link <id> <id>             # related
nv note search "<query>" [filters]

nv commitment today                # two lists, see below
nv commitment done <id>            # "Done #12"
nv commitment drop <id>            # "Dropped #12"
nv commitment postpone <id> <date> # "Postponed #12 to Fri 2026-10-09"; also sets a first date

nv people add "<name>" [--role R] [--alias A]...
nv people edit <id> [--name N] [--role R]
nv people list
nv people search "<text>"
nv people alias <id> "<alias>"
nv people merge <keep-id> <other-id>   # by ID: names are not unique

nv history [--note <id>] [--person <id>] [--limit N]   # change log, newest first
nv history undo [<change-id>]      # without an ID: the newest change that still stands;
                                   # only the last change of a note or person can be undone

nv model reindex                   # embed every note again
nv model info                      # model, folder, embedded and pending counts
nv model embed-pending             # hidden; started by nv add and nv note edit

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
     We agreed with Anna to use 5 retries.
     project: Billing · repos: billing-api · tickets: PAY-1234 · people: Anna Nowak

#17  decision · work · 2026-09-12 · outdated → #42
     Retry 3 times
     We retry billing calls 3 times.

showing 2 of 7, use --limit
```

Today view (`--json` gives `today`, `mine`, `owed` with `owner_name`, and `undated`):

```
Planned for today (2026-10-07)
#2  Book the exam slot · planned 2026-10-05, overdue
#1  Send retry numbers to Anna

Others owe you
#3  Anna Nowak: Review the retry PR · planned 2026-10-08
#4  Piotr Zielinski: Send the staging access steps

2 more of yours have no date: nv search --type commitment --status todo
```

Nothing to show: `Nothing planned for today.`

History:

```
#5  2026-10-07 09:00  claude  edit        note #2  Retry 5 times for billing-api  (undone)
#4  2026-10-06 10:30  user    replace     note #1  Retry 3 times
#2  2026-10-05 09:00  claude  person-add  person #1  Anna Nowak
```

- A note with no type shows `note`. A commitment shows `todo`, `done` or `dropped` in place of `active`.
- The details line holds owner, planned date, project, repos, tickets, people, source, expiry date, `replaces: #17` and `related: #5, #9`, only those that are set. A commitment of mine shows no owner.
- A search result shows the first line of the body under the title, cut at 100 characters. `nv note show` prints the header, title and details, then an empty line and the whole body.
- `showing 2 of 7, use --limit` ends a search with only filters when there are more notes than the limit. A text search never has this line.
- `nv add` answers `Saved #42` (`Edited #42`, `Deleted #42`), followed by `, planned Thu 2026-10-08` and `, expires Sat 2026-11-14` when the note has those dates; with `--json`, the whole note.
- No results: `No notes found.` Search `--json` is a list of notes, each with its `rank`.
- Exit codes: 0 ok, 1 nv refused or failed (not found, broken rule, secret), 2 wrong usage.

**Edit:** each flag replaces that field; `--repo`, `--ticket` and `--person` replace the whole list, while `--add-repo`, `--remove-repo` and the same for ticket and person change one value. `--planned` moves the date of a todo commitment. The body changes only with `--body`, which reads the new body from stdin. A field cannot be cleared.

**Replace:** `--title` and the body are new; area, type, project, repos, tickets, people and source are copied from the old note unless a flag gives them. A commitment keeps owner and planned date. The expiry date is not copied.

**Search filters**, all combinable: `--area`, `--type`, `--status todo|done|dropped` (commitments only), `--project` (whole name, any case), `--repo`, `--ticket`, `--person <id>` (notes about the person and commitments they own), `--since <date>`, `--planned <date>` (commitments planned for that date), `--all` (include expired), `--limit` (default 5). A search with only filters and no text is allowed, for example `nv search --ticket PAY-1234`; it does not load the model.

**People** are always addressed by ID; `nv people search` shows every match with aliases and role, so Claude sees which Anna before it links or merges.

```
#1  Anna Nowak · QA lead
    aliases: Anna, anna.nowak@contoso.com
#3  Anna Kowalska · backend developer
    aliases: Anna, Ania
```

## Database tables

This schema follows the three contexts and the aggregate rules from session 2.

```sql
-- Knowledge: every note, commitments included
CREATE TABLE notes (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,  -- an ID is never reused
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
CREATE TABLE people       (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE, role TEXT);
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
CREATE VIRTUAL TABLE notes_fts USING fts5(title, body, content='notes', content_rowid='id',
  tokenize='porter unicode61');   -- kept in step with notes by triggers (migration 0002)

-- Shared
CREATE TABLE change_log (
  id INTEGER PRIMARY KEY, at TEXT NOT NULL,   -- injected, no SQL default
  actor TEXT NOT NULL,            -- 'claude' or 'user'
  action TEXT NOT NULL,           -- 'add', 'edit', 'delete', 'restore', 'done', 'drop', 'postpone',
                                  -- 'replace', 'link', 'person-add', 'person-edit', 'alias',
                                  -- 'merge', 'undo'
  note_id INTEGER, person_id INTEGER,   -- a change is about a note or about a person
  CHECK (note_id IS NOT NULL OR person_id IS NOT NULL),
  before_json TEXT,               -- old state, used by undo
  undone_at TEXT                  -- set when the change was undone; the line stays
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
- [x] Build the CLI: skeleton and schema, notes and change log, embeddings and hybrid search, commitments, people, links, replace, history and undo
- [x] Write the Claude Code skills: `nv-recall` (when and how to search) and `nv-capture` (when to save, saving rules, template), in `claude/skills/`
- [ ] Install it (`./install.sh`) and run the hand test in a real session (`docs/skill-test.md`)
- [ ] Build and test on the work laptop's OS; check `nv` is a free command name there
- [ ] Check company rules for using Claude Code with project data

**Later, not MVP**

- Weekday check: `nv add` and `nv note edit` refuse a title or body where a weekday name stands next to a date that is a different day ("Friday 2026-10-11" → "2026-10-11 is a Sunday"). A safety net like the secret check. From the hand test on 2026-10-07: Haiku wrote that date in a body, where nv could not see it
- Date helper: a command such as `nv date` that prints today and the next 14 days with their weekdays, so that Claude looks a date up instead of calculating it. The skill would say so. From the same hand test: small models get "Friday" wrong and the skills allow only `nv`, so they cannot run `date`
- Notifications for planned commitments
- Conflict check when saving a new note
- Approval step before saving
- Background process that keeps the model loaded
- MCP server on top of the CLI
- `nv export --area personal`

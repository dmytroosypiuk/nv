# nv

nv is a local, offline knowledge store for Claude Code. Claude saves short notes
(decisions, commitments, how-tos, facts, ideas) about work, learning and personal life,
and finds them again later with keyword + semantic search.

The full design is in `docs/design.md`. Read it before any non-trivial change.

## Hard rules

- **Offline at runtime.** nv must never make network calls when it runs. The embedding
  model is loaded from a local folder. Downloads are allowed only at build/setup time.
- **Rust, single binary.** Ship `nv` + a `models/` folder. No Python, no runtime installs.
- **SQLite** via `rusqlite` with the `bundled` feature. Keyword search with FTS5.
- **Embeddings:** `bge-small-en-v1.5` through `fastembed-rs`, loaded from local files
  (user-defined model API). 384 dimensions, normalized vectors.
- **Vector search:** vectors stored as BLOBs, brute-force cosine (dot product). Do not
  add `sqlite-vec` or any vector database.
- **Notes are always in English.** Claude writes notes and search queries in English.
- **Never store secrets.** `nv add` must refuse text that looks like a password, token or
  key (`ghp_…`, `sk-…`, `password=…`, long random strings).
- **Nothing is lost silently.** Every edit, replace and delete goes to `change_log`, so
  `nv history undo` works.

## Words (ubiquitous language)

Use these exact words in code, CLI and messages. Full definitions in `docs/design.md`.

- **note**: small piece of information for later. Title + body + area are required.
- **type**: zero or one of `decision`, `commitment`, `how-to`, `fact`, `idea`.
- **area**: exactly one of `work`, `learning`, `personal`.
- **commitment**: a note where someone promised an action. Has owner (NULL = me),
  status `todo` / `done` / `dropped`, optional planned date. `done` and `dropped` are final.
- **source**: where a note came from: kind (`meeting`, `chat`, `email`, `ticket`, `web`,
  `repo`, `doc`) + reference. Optional.
- **outdated**: was true, a newer note replaced it (link kind `replaced_by`).
- **expired**: its `expires_on` date passed. Hidden from search unless `--all`.
- **link**: `replaced_by` or `related`. A note can never link to itself.
- **person**: one main name + many aliases. Always addressed by ID.

Do not use the old words "point" or "vault".

## CLI shape

Docker-style: object first, then action (`nv note add`, `nv people merge 7 12`).
Shortcuts: `nv add`, `nv search`, `nv today`.
Input: fields as flags, body from stdin. Output: compact text by default, `--json` flag.
See the "CLI commands" section of `docs/design.md` for the full tree.

## Code layout (target)

Three bounded contexts, kept as separate modules:

- `knowledge` – notes, links, sources, people, change log (owns the data)
- `commitments` – rules for commitment notes (status changes, postpone, today view)
- `search` – embeddings, model, FTS, ranking. Depends on `knowledge`, never the reverse.

## How to work: TDD

- Test first, always. Red (a failing test for one behaviour) → green (smallest code that
  passes) → refactor (clean names, using the words above). No production code without a
  failing test.
- Domain rules are tested in Rust, not only by SQL CHECKs.
- Search depends on an `Embedder` trait; unit tests use a fake. Tests that load the real
  model are `#[ignore]` and run with `cargo test -- --ignored`.
- Time is injected, never read inside domain code.
- CLI tests use `assert_cmd` with a temporary `NV_HOME`.
- Use plan mode for bigger steps: show the plan and the list of tests before writing code.

## Before finishing any task

- `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` all pass.
- Explain in the final message what changed and anything left open.

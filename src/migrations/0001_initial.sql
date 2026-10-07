-- Knowledge: every note, commitments included
CREATE TABLE people (
  id   INTEGER PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  role TEXT
);
CREATE TABLE person_aliases (
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  alias     TEXT NOT NULL,
  PRIMARY KEY (person_id, alias)      -- short aliases like 'Anna' may repeat
);
CREATE TABLE notes (
  id          INTEGER PRIMARY KEY,
  title       TEXT NOT NULL,
  body        TEXT NOT NULL,
  area        TEXT NOT NULL CHECK (area IN ('work','learning','personal')),
  type        TEXT CHECK (type IN ('decision','commitment','how-to','fact','idea')),
  project     TEXT,
  status      TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','outdated')),
  source_kind TEXT CHECK (source_kind IN ('meeting','chat','email','ticket','web','repo','doc')),
  source_ref  TEXT,
  -- Commitments context: only for type = 'commitment'
  commitment_status TEXT CHECK (commitment_status IN ('todo','done','dropped')),
  owner_person_id   INTEGER REFERENCES people(id),  -- NULL = me
  planned_for       TEXT,
  expires_on        TEXT,             -- any note: hidden from search after this date
  closed_at         TEXT,
  -- time is injected by nv, never defaulted here
  created_at  TEXT NOT NULL,
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
CREATE TABLE note_repos (
  note_id INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  repo    TEXT NOT NULL,
  PRIMARY KEY (note_id, repo)
);
CREATE TABLE note_tickets (
  note_id   INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  ticket_id TEXT NOT NULL,
  PRIMARY KEY (note_id, ticket_id)
);
CREATE TABLE note_people (
  note_id   INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  PRIMARY KEY (note_id, person_id)
);

-- Search (fed by notes)
CREATE TABLE embeddings (
  note_id   INTEGER NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
  model     TEXT NOT NULL,            -- 'bge-small-en-v1.5'
  dims      INTEGER NOT NULL,         -- 384
  vector    BLOB NOT NULL,
  text_hash TEXT NOT NULL,            -- re-embed when the text changes
  PRIMARY KEY (note_id, model)
);
CREATE VIRTUAL TABLE notes_fts USING fts5(title, body, content='notes', content_rowid='id');

-- Shared
CREATE TABLE change_log (
  id          INTEGER PRIMARY KEY,
  at          TEXT NOT NULL,
  actor       TEXT NOT NULL,          -- 'claude' or 'user'
  action      TEXT NOT NULL,
  note_id     INTEGER NOT NULL,
  before_json TEXT                    -- old state, used by undo
);

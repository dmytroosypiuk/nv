-- A change can be about a person (add, edit, alias, merge), not only about a note.
CREATE TABLE change_log_new (
  id          INTEGER PRIMARY KEY,
  at          TEXT NOT NULL,
  actor       TEXT NOT NULL,          -- 'claude' or 'user'
  action      TEXT NOT NULL,
  note_id     INTEGER,
  person_id   INTEGER,
  before_json TEXT,                   -- old state, used by undo
  CHECK (note_id IS NOT NULL OR person_id IS NOT NULL)
);
INSERT INTO change_log_new (id, at, actor, action, note_id, before_json)
  SELECT id, at, actor, action, note_id, before_json FROM change_log;
DROP TABLE change_log;
ALTER TABLE change_log_new RENAME TO change_log;

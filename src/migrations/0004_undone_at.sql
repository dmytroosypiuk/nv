-- An undone change stays in the log, marked with the time of the undo.
ALTER TABLE change_log ADD COLUMN undone_at TEXT;

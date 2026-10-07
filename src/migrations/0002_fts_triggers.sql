-- Keep the keyword index in step with notes.
CREATE TRIGGER notes_fts_after_insert AFTER INSERT ON notes BEGIN
  INSERT INTO notes_fts (rowid, title, body) VALUES (new.id, new.title, new.body);
END;
CREATE TRIGGER notes_fts_after_delete AFTER DELETE ON notes BEGIN
  INSERT INTO notes_fts (notes_fts, rowid, title, body)
  VALUES ('delete', old.id, old.title, old.body);
END;
CREATE TRIGGER notes_fts_after_update AFTER UPDATE OF title, body ON notes BEGIN
  INSERT INTO notes_fts (notes_fts, rowid, title, body)
  VALUES ('delete', old.id, old.title, old.body);
  INSERT INTO notes_fts (rowid, title, body) VALUES (new.id, new.title, new.body);
END;

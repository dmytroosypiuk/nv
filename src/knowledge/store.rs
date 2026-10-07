//! Saves and loads notes in SQLite. Every change goes to the change log.

use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};

use super::change_log::{self, Action, Actor};
use super::note::{Note, NoteChanges, NoteDraft, NoteStatus, Source};
use crate::clock::Now;

pub struct NoteStore<'c> {
    conn: &'c Connection,
}

impl<'c> NoteStore<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn add(&self, draft: &NoteDraft, actor: Actor, now: &Now) -> Result<Note> {
        let mut note = Note {
            id: 0,
            title: draft.title.clone(),
            body: draft.body.clone(),
            area: draft.area,
            note_type: draft.note_type,
            project: draft.project.clone(),
            status: NoteStatus::Active,
            commitment_status: draft.commitment_status(),
            source: draft.source.clone(),
            repos: draft.repos.clone(),
            tickets: draft.tickets.clone(),
            expires_on: draft.expires_on,
            created_at: now.timestamp(),
            updated_at: now.timestamp(),
        };
        let tx = self.conn.unchecked_transaction()?;
        note.id = insert(&tx, &note, None)?;
        change_log::record(&tx, now, actor, Action::Add, note.id, None)?;
        tx.commit()?;
        Ok(note)
    }

    pub fn get(&self, id: i64) -> Result<Option<Note>> {
        type Row = (
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            String,
            String,
        );
        let row: Option<Row> = self
            .conn
            .query_row(
                "SELECT title, body, area, type, project, status, commitment_status,
                        source_kind, source_ref, expires_on, created_at, updated_at
                 FROM notes WHERE id = ?1",
                [id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            title,
            body,
            area,
            note_type,
            project,
            status,
            commitment_status,
            source_kind,
            source_ref,
            expires_on,
            created_at,
            updated_at,
        )) = row
        else {
            return Ok(None);
        };
        let source_kind = source_kind.map(|kind| kind.parse()).transpose()?;
        Ok(Some(Note {
            id,
            title,
            body,
            area: area.parse()?,
            note_type: note_type.map(|word| word.parse()).transpose()?,
            project,
            status: status.parse()?,
            commitment_status: commitment_status.map(|word| word.parse()).transpose()?,
            source: Source::from_parts(source_kind, source_ref)?,
            repos: self.words(
                "SELECT repo FROM note_repos WHERE note_id = ?1 ORDER BY rowid",
                id,
            )?,
            tickets: self.words(
                "SELECT ticket_id FROM note_tickets WHERE note_id = ?1 ORDER BY rowid",
                id,
            )?,
            expires_on: expires_on.map(|date| date.parse()).transpose()?,
            created_at,
            updated_at,
        }))
    }

    pub fn edit(&self, id: i64, changes: &NoteChanges, actor: Actor, now: &Now) -> Result<Note> {
        let before = self.existing(id)?;
        let mut after = before.edited(changes)?;
        after.updated_at = now.timestamp();

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "UPDATE notes SET title = ?2, body = ?3, area = ?4, type = ?5, project = ?6,
                    commitment_status = ?7, source_kind = ?8, source_ref = ?9, updated_at = ?10,
                    expires_on = ?11
             WHERE id = ?1",
            params![
                id,
                after.title,
                after.body,
                after.area.as_str(),
                after.note_type.map(|word| word.as_str()),
                after.project,
                after.commitment_status.map(|word| word.as_str()),
                after.source.as_ref().map(|source| source.kind.as_str()),
                after
                    .source
                    .as_ref()
                    .map(|source| source.reference.as_str()),
                after.updated_at,
                after.expires_on.map(|date| date.to_string()),
            ],
        )?;
        tx.execute("DELETE FROM note_repos WHERE note_id = ?1", [id])?;
        tx.execute("DELETE FROM note_tickets WHERE note_id = ?1", [id])?;
        insert_repos_and_tickets(&tx, &after)?;
        change_log::record(&tx, now, actor, Action::Edit, id, Some(&before))?;
        tx.commit()?;
        Ok(after)
    }

    /// Removes the note; the change log keeps it. Returns the deleted note.
    pub fn delete(&self, id: i64, actor: Actor, now: &Now) -> Result<Note> {
        let before = self.existing(id)?;
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM notes WHERE id = ?1", [id])?;
        change_log::record(&tx, now, actor, Action::Delete, id, Some(&before))?;
        tx.commit()?;
        Ok(before)
    }

    /// Brings back the note that change `change_id` deleted, with its old ID.
    pub fn restore(&self, change_id: i64, actor: Actor, now: &Now) -> Result<Note> {
        let change = change_log::change(self.conn, change_id)?
            .ok_or_else(|| anyhow!("change #{change_id} not found"))?;
        let note = match (change.action, change.before) {
            (Action::Delete, Some(note)) => note,
            _ => bail!("change #{change_id} is not a deletion"),
        };
        if self.get(note.id)?.is_some() {
            bail!("note #{} already exists", note.id);
        }
        let tx = self.conn.unchecked_transaction()?;
        insert(&tx, &note, Some(note.id))?;
        change_log::record(&tx, now, actor, Action::Restore, note.id, None)?;
        tx.commit()?;
        Ok(note)
    }

    fn existing(&self, id: i64) -> Result<Note> {
        self.get(id)?.ok_or_else(|| anyhow!("note #{id} not found"))
    }

    fn words(&self, sql: &str, note_id: i64) -> Result<Vec<String>> {
        let mut statement = self.conn.prepare(sql)?;
        let words = statement.query_map([note_id], |row| row.get(0))?;
        Ok(words.collect::<rusqlite::Result<_>>()?)
    }
}

/// Inserts the note under `id`, or under a new ID when `id` is `None`. Returns the ID.
fn insert(conn: &Connection, note: &Note, id: Option<i64>) -> Result<i64> {
    conn.execute(
        "INSERT INTO notes (id, title, body, area, type, project, status, commitment_status,
                            source_kind, source_ref, created_at, updated_at, expires_on)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        params![
            id,
            note.title,
            note.body,
            note.area.as_str(),
            note.note_type.map(|word| word.as_str()),
            note.project,
            note.status.as_str(),
            note.commitment_status.map(|word| word.as_str()),
            note.source.as_ref().map(|source| source.kind.as_str()),
            note.source.as_ref().map(|source| source.reference.as_str()),
            note.created_at,
            note.updated_at,
            note.expires_on.map(|date| date.to_string()),
        ],
    )?;
    let id = conn.last_insert_rowid();
    insert_repos_and_tickets(conn, &Note { id, ..note.clone() })?;
    Ok(id)
}

fn insert_repos_and_tickets(conn: &Connection, note: &Note) -> Result<()> {
    for repo in &note.repos {
        conn.execute(
            "INSERT INTO note_repos (note_id, repo) VALUES (?1, ?2)",
            params![note.id, repo],
        )?;
    }
    for ticket in &note.tickets {
        conn.execute(
            "INSERT INTO note_tickets (note_id, ticket_id) VALUES (?1, ?2)",
            params![note.id, ticket],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

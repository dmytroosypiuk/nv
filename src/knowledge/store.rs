//! Saves and loads notes in SQLite. Every change goes to the change log.

use anyhow::{Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, named_params};

use super::change_log::{self, Action, Actor};
use super::note::{Note, NoteChanges, NoteDraft, NoteStatus, Source};
use super::people::person_name;
use crate::clock::Now;

pub struct NoteStore<'c> {
    conn: &'c Connection,
}

impl<'c> NoteStore<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn add(&self, draft: &NoteDraft, actor: Actor, now: &Now) -> Result<Note> {
        self.in_transaction(|| self.add_unlogged_tx(draft, actor, now))
    }

    /// `add` without its own transaction, for commands that do more in one transaction.
    fn add_unlogged_tx(&self, draft: &NoteDraft, actor: Actor, now: &Now) -> Result<Note> {
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
            people: draft.people.clone(),
            expires_on: draft.expires_on,
            owner: draft.owner,
            planned_for: draft.planned_for,
            closed_at: None,
            replaced_by: None,
            replaces: Vec::new(),
            related: Vec::new(),
            created_at: now.timestamp(),
            updated_at: now.timestamp(),
        };
        self.check_people(&note)?;
        note.id = insert(self.conn, &note, None)?;
        change_log::record(self.conn, now, actor, Action::Add, note.id, None)?;
        Ok(note)
    }

    pub fn get(&self, id: i64) -> Result<Option<Note>> {
        /// The columns of `notes` as SQLite gives them, before the words are parsed.
        struct Row {
            title: String,
            body: String,
            area: String,
            note_type: Option<String>,
            project: Option<String>,
            status: String,
            commitment_status: Option<String>,
            source_kind: Option<String>,
            source_ref: Option<String>,
            expires_on: Option<String>,
            owner: Option<i64>,
            planned_for: Option<String>,
            closed_at: Option<String>,
            created_at: String,
            updated_at: String,
        }
        let row = self
            .conn
            .query_row("SELECT * FROM notes WHERE id = ?1", [id], |row| {
                Ok(Row {
                    title: row.get("title")?,
                    body: row.get("body")?,
                    area: row.get("area")?,
                    note_type: row.get("type")?,
                    project: row.get("project")?,
                    status: row.get("status")?,
                    commitment_status: row.get("commitment_status")?,
                    source_kind: row.get("source_kind")?,
                    source_ref: row.get("source_ref")?,
                    expires_on: row.get("expires_on")?,
                    owner: row.get("owner_person_id")?,
                    planned_for: row.get("planned_for")?,
                    closed_at: row.get("closed_at")?,
                    created_at: row.get("created_at")?,
                    updated_at: row.get("updated_at")?,
                })
            })
            .optional()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let source_kind = row.source_kind.map(|kind| kind.parse()).transpose()?;
        Ok(Some(Note {
            id,
            title: row.title,
            body: row.body,
            area: row.area.parse()?,
            note_type: row.note_type.map(|word| word.parse()).transpose()?,
            project: row.project,
            status: row.status.parse()?,
            commitment_status: row.commitment_status.map(|word| word.parse()).transpose()?,
            source: Source::from_parts(source_kind, row.source_ref)?,
            repos: self.words(
                "SELECT repo FROM note_repos WHERE note_id = ?1 ORDER BY rowid",
                id,
            )?,
            tickets: self.words(
                "SELECT ticket_id FROM note_tickets WHERE note_id = ?1 ORDER BY rowid",
                id,
            )?,
            people: {
                let mut statement = self.conn.prepare(
                    "SELECT person_id FROM note_people WHERE note_id = ?1 ORDER BY rowid",
                )?;
                let people = statement.query_map([id], |row| row.get(0))?;
                people.collect::<rusqlite::Result<_>>()?
            },
            expires_on: row.expires_on.map(|date| date.parse()).transpose()?,
            owner: row.owner,
            planned_for: row.planned_for.map(|date| date.parse()).transpose()?,
            closed_at: row.closed_at,
            replaced_by: self
                .note_ids(
                    "SELECT to_id FROM note_links WHERE from_id = ?1 AND kind = 'replaced_by'",
                    id,
                )?
                .pop(),
            replaces: self.note_ids(
                "SELECT from_id FROM note_links WHERE to_id = ?1 AND kind = 'replaced_by'
                 ORDER BY from_id",
                id,
            )?,
            // A related link has no direction: it is stored once and read from both ends.
            related: self.note_ids(
                "SELECT to_id AS other FROM note_links WHERE from_id = ?1 AND kind = 'related'
                 UNION
                 SELECT from_id FROM note_links WHERE to_id = ?1 AND kind = 'related'
                 ORDER BY other",
                id,
            )?,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }))
    }

    pub fn edit(&self, id: i64, changes: &NoteChanges, actor: Actor, now: &Now) -> Result<Note> {
        self.change(id, Action::Edit, actor, now, |note| {
            Ok(note.edited(changes)?)
        })
    }

    /// Loads the note, applies `rule` to it, saves the result and logs `action` with the
    /// state before. Nothing is saved when the rule refuses.
    pub fn change(
        &self,
        id: i64,
        action: Action,
        actor: Actor,
        now: &Now,
        rule: impl FnOnce(&Note) -> Result<Note>,
    ) -> Result<Note> {
        self.in_transaction(|| self.apply(id, action, actor, now, rule))
    }

    /// `change` without its own transaction.
    pub(super) fn apply(
        &self,
        id: i64,
        action: Action,
        actor: Actor,
        now: &Now,
        rule: impl FnOnce(&Note) -> Result<Note>,
    ) -> Result<Note> {
        let before = self.existing(id)?;
        let mut after = rule(&before)?;
        after.updated_at = now.timestamp();
        after.check_links()?;
        self.check_people(&after)?;

        let conn = self.conn;
        conn.execute(
            "UPDATE notes SET
                title = :title, body = :body, area = :area, type = :type, project = :project,
                status = :status, commitment_status = :commitment_status,
                source_kind = :source_kind, source_ref = :source_ref,
                expires_on = :expires_on, owner_person_id = :owner,
                planned_for = :planned_for, closed_at = :closed_at, updated_at = :updated_at
             WHERE id = :id",
            named_params! {
                ":id": id,
                ":title": after.title,
                ":body": after.body,
                ":area": after.area.as_str(),
                ":type": after.note_type.map(|word| word.as_str()),
                ":project": after.project,
                ":status": after.status.as_str(),
                ":commitment_status": after.commitment_status.map(|word| word.as_str()),
                ":source_kind": after.source.as_ref().map(|source| source.kind.as_str()),
                ":source_ref": after.source.as_ref().map(|source| source.reference.as_str()),
                ":expires_on": after.expires_on.map(|date| date.to_string()),
                ":owner": after.owner,
                ":planned_for": after.planned_for.map(|date| date.to_string()),
                ":closed_at": after.closed_at,
                ":updated_at": after.updated_at,
            },
        )?;
        conn.execute("DELETE FROM note_repos WHERE note_id = ?1", [id])?;
        conn.execute("DELETE FROM note_tickets WHERE note_id = ?1", [id])?;
        conn.execute("DELETE FROM note_people WHERE note_id = ?1", [id])?;
        insert_repos_tickets_and_people(conn, &after)?;
        write_links(conn, &after)?;
        change_log::record(conn, now, actor, action, id, Some(&before))?;
        Ok(after)
    }

    /// Saves a newer note that says what is true now: the old note becomes outdated and
    /// gets a `replaced_by` link to it. Returns the new note.
    pub fn replace(&self, old_id: i64, draft: &NoteDraft, actor: Actor, now: &Now) -> Result<Note> {
        self.in_transaction(|| {
            self.existing(old_id)?;
            let new = self.add_unlogged_tx(draft, actor, now)?;
            self.apply(old_id, Action::Replace, actor, now, |old| {
                Ok(old.replaced_by_note(new.id)?)
            })?;
            self.existing(new.id)
        })
    }

    /// Links two notes as related. Returns false when they were linked already.
    pub fn link(&self, id: i64, other_id: i64, actor: Actor, now: &Now) -> Result<bool> {
        let note = self.existing(id)?;
        let linked = note.linked_to(other_id)?;
        let other = self.existing(other_id)?;
        if linked == note {
            return Ok(false);
        }
        // Logged on both notes, each with its state before: the link changes both.
        self.in_transaction(|| {
            self.apply(id, Action::Link, actor, now, |_| Ok(linked))?;
            self.conn.execute(
                "UPDATE notes SET updated_at = ?2 WHERE id = ?1",
                rusqlite::params![other_id, now.timestamp()],
            )?;
            change_log::record(self.conn, now, actor, Action::Link, other_id, Some(&other))?;
            Ok(true)
        })
    }

    /// Removes the note; the change log keeps it. Returns the deleted note.
    pub fn delete(&self, id: i64, actor: Actor, now: &Now) -> Result<Note> {
        let before = self.existing(id)?;
        if let Some(replaced) = before.replaces.first() {
            // Otherwise that note would stay outdated with nothing saying what is true now.
            bail!("note #{id} replaced #{replaced}: undo the replace, or replace #{id}");
        }
        self.in_transaction(|| {
            self.remove(id)?;
            change_log::record(self.conn, now, actor, Action::Delete, id, Some(&before))?;
            Ok(before)
        })
    }

    /// Brings back the note that change `change_id` deleted, with its old ID.
    pub fn restore(&self, change_id: i64, actor: Actor, now: &Now) -> Result<Note> {
        let change = change_log::change(self.conn, change_id)?
            .ok_or_else(|| anyhow!("change #{change_id} not found"))?;
        let note = match (change.action, change.note_before()?) {
            (Action::Delete, Some(note)) => note,
            _ => bail!("change #{change_id} is not a deletion"),
        };
        self.in_transaction(|| {
            let note = self.put_back(&note)?;
            change_log::record(self.conn, now, actor, Action::Restore, note.id, None)?;
            Ok(note)
        })
    }

    /// Deletes the row of a note, with nothing written to the change log.
    pub(super) fn remove(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM notes WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Inserts a note that is not in the database under its old ID, with its links to
    /// notes that still exist. Nothing is written to the change log.
    pub(super) fn put_back(&self, note: &Note) -> Result<Note> {
        if self.get(note.id)?.is_some() {
            bail!("note #{} already exists", note.id);
        }
        let mut note = note.clone();
        // The newer note may be gone; then this note is what is true again.
        if let Some(newer) = note.replaced_by
            && self.get(newer)?.is_none()
        {
            note.replaced_by = None;
            note.status = NoteStatus::Active;
        }
        self.check_people(&note)?;
        insert(self.conn, &note, Some(note.id))?;
        write_links(self.conn, &note)?;
        self.existing(note.id)
    }

    /// Runs `work` in one transaction: all of it is saved, or nothing.
    pub(super) fn in_transaction<T>(&self, work: impl FnOnce() -> Result<T>) -> Result<T> {
        let tx = self.conn.unchecked_transaction()?;
        let result = work()?;
        tx.commit()?;
        Ok(result)
    }

    fn note_ids(&self, sql: &str, note_id: i64) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(sql)?;
        let ids = statement.query_map([note_id], |row| row.get(0))?;
        Ok(ids.collect::<rusqlite::Result<_>>()?)
    }

    fn existing(&self, id: i64) -> Result<Note> {
        self.get(id)?.ok_or_else(|| anyhow!("note #{id} not found"))
    }

    /// The owner of a commitment and the people of a note must be known people.
    fn check_people(&self, note: &Note) -> Result<()> {
        for &person in note.owner.iter().chain(&note.people) {
            if person_name(self.conn, person)?.is_none() {
                bail!("person #{person} not found");
            }
        }
        Ok(())
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
                            source_kind, source_ref, expires_on, owner_person_id,
                            planned_for, closed_at, created_at, updated_at)
         VALUES (:id, :title, :body, :area, :type, :project, :status, :commitment_status,
                 :source_kind, :source_ref, :expires_on, :owner,
                 :planned_for, :closed_at, :created_at, :updated_at)",
        named_params! {
            ":id": id,
            ":title": note.title,
            ":body": note.body,
            ":area": note.area.as_str(),
            ":type": note.note_type.map(|word| word.as_str()),
            ":project": note.project,
            ":status": note.status.as_str(),
            ":commitment_status": note.commitment_status.map(|word| word.as_str()),
            ":source_kind": note.source.as_ref().map(|source| source.kind.as_str()),
            ":source_ref": note.source.as_ref().map(|source| source.reference.as_str()),
            ":expires_on": note.expires_on.map(|date| date.to_string()),
            ":owner": note.owner,
            ":planned_for": note.planned_for.map(|date| date.to_string()),
            ":closed_at": note.closed_at,
            ":created_at": note.created_at,
            ":updated_at": note.updated_at,
        },
    )?;
    let id = conn.last_insert_rowid();
    insert_repos_tickets_and_people(conn, &Note { id, ..note.clone() })?;
    Ok(id)
}

/// Writes the outgoing `replaced_by` link and the `related` links of a note. Links to
/// notes that do not exist are left out.
fn write_links(conn: &Connection, note: &Note) -> Result<()> {
    conn.execute(
        "DELETE FROM note_links
         WHERE (from_id = ?1 AND kind = 'replaced_by')
            OR (kind = 'related' AND (from_id = ?1 OR to_id = ?1))",
        [note.id],
    )?;
    if let Some(newer) = note.replaced_by {
        conn.execute(
            "INSERT INTO note_links (from_id, to_id, kind)
             SELECT ?1, id, 'replaced_by' FROM notes WHERE id = ?2",
            rusqlite::params![note.id, newer],
        )?;
    }
    for &other in &note.related {
        // Stored once, from the smaller ID to the bigger one.
        conn.execute(
            "INSERT OR IGNORE INTO note_links (from_id, to_id, kind)
             SELECT ?1, ?2, 'related' WHERE EXISTS (SELECT 1 FROM notes WHERE id = ?3)",
            rusqlite::params![note.id.min(other), note.id.max(other), other],
        )?;
    }
    Ok(())
}

fn insert_repos_tickets_and_people(conn: &Connection, note: &Note) -> Result<()> {
    for repo in &note.repos {
        conn.execute(
            "INSERT INTO note_repos (note_id, repo) VALUES (?1, ?2)",
            rusqlite::params![note.id, repo],
        )?;
    }
    for ticket in &note.tickets {
        conn.execute(
            "INSERT INTO note_tickets (note_id, ticket_id) VALUES (?1, ?2)",
            rusqlite::params![note.id, ticket],
        )?;
    }
    for person in &note.people {
        conn.execute(
            "INSERT INTO note_people (note_id, person_id) VALUES (?1, ?2)",
            rusqlite::params![note.id, person],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;

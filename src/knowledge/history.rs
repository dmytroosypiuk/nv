//! Reading the change log, and undoing changes.

use anyhow::{Result, anyhow, bail};
use rusqlite::Connection;
use serde::Serialize;

use super::change_log::{self, Action, Actor, Change};
use super::note::Note;
use super::people::{PeopleStore, person_name};
use super::person::Person;
use super::store::NoteStore;
use crate::clock::Now;

/// One line of `nv history`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub at: String,
    pub actor: Actor,
    pub action: Action,
    /// "note" or "person".
    pub subject: &'static str,
    pub subject_id: i64,
    /// The title of the note or the name of the person, also when it is gone.
    pub title: Option<String>,
    pub undone: bool,
}

/// Which changes to show. Without a filter: all.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HistoryFilter {
    pub note: Option<i64>,
    pub person: Option<i64>,
}

/// The change log, newest first.
pub fn history(
    conn: &Connection,
    filter: HistoryFilter,
    limit: usize,
) -> Result<Vec<HistoryEntry>> {
    let mut entries = Vec::new();
    for change in change_log::recent(conn, filter.note, filter.person, limit)? {
        let (subject, subject_id, title) = match (change.note_id, change.person_id) {
            (Some(note_id), _) => ("note", note_id, note_title(conn, note_id)?),
            (None, Some(person_id)) => ("person", person_id, name_of_person(conn, person_id)?),
            (None, None) => bail!("change #{} is about nothing", change.id),
        };
        entries.push(HistoryEntry {
            id: change.id,
            at: change.at,
            actor: change.actor,
            action: change.action,
            subject,
            subject_id,
            title,
            undone: change.undone_at.is_some(),
        });
    }
    Ok(entries)
}

/// The title of a note; for a note that is gone, its last title in the change log.
fn note_title(conn: &Connection, note_id: i64) -> Result<Option<String>> {
    if let Some(note) = NoteStore::new(conn).get(note_id)? {
        return Ok(Some(note.title));
    }
    for change in change_log::changes_of_note(conn, note_id)?.iter().rev() {
        if let Some(note) = change.note_before()? {
            return Ok(Some(note.title));
        }
    }
    Ok(None)
}

/// The name of a person; for a person that is gone, their last name in the change log.
fn name_of_person(conn: &Connection, person_id: i64) -> Result<Option<String>> {
    if let Some(name) = person_name(conn, person_id)? {
        return Ok(Some(name));
    }
    for change in change_log::changes_of_person(conn, person_id)?.iter().rev() {
        let before = change
            .before_json
            .as_deref()
            .map(serde_json::from_str::<Person>);
        if let Some(Ok(person)) = before {
            return Ok(Some(person.name));
        }
    }
    Ok(None)
}

/// What an undo did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoOutcome {
    /// The change that was undone.
    pub change: Change,
    /// Like "edit of note #12".
    pub description: String,
    /// Things undo could not put back, like a person that no longer exists.
    pub warnings: Vec<String>,
    /// True when note text changed or came back, so embeddings are pending.
    pub text_changed: bool,
}

/// Undoes one change; without an ID, the newest change that still stands.
/// Only the last change of a note or a person can be undone.
pub fn undo(
    conn: &Connection,
    change_id: Option<i64>,
    actor: Actor,
    now: &Now,
) -> Result<UndoOutcome> {
    let change = match change_id {
        Some(id) => {
            change_log::change(conn, id)?.ok_or_else(|| anyhow!("change #{id} not found"))?
        }
        None => change_log::recent(conn, None, None, usize::MAX >> 1)?
            .into_iter()
            .find(Change::stands)
            .ok_or_else(|| anyhow!("nothing to undo"))?,
    };
    if matches!(change.action, Action::Undo | Action::Restore) {
        bail!("change #{} is an undo: it cannot be undone", change.id);
    }
    if change.undone_at.is_some() {
        bail!("change #{} is already undone", change.id);
    }

    // One transaction: the state, the mark on the change and the undo entry, or nothing.
    let tx = conn.unchecked_transaction()?;
    let undoing = Undoing {
        conn,
        change: &change,
        actor,
        now,
    };
    let mut outcome = match (change.note_id, change.person_id) {
        (Some(note_id), _) => undoing.note_change(note_id)?,
        (None, Some(person_id)) => undoing.person_change(person_id)?,
        (None, None) => bail!("change #{} is about nothing", change.id),
    };
    change_log::mark_undone(conn, change.id, now)?;
    tx.commit()?;
    outcome.change = change.clone();
    Ok(outcome)
}

struct Undoing<'a> {
    conn: &'a Connection,
    change: &'a Change,
    actor: Actor,
    now: &'a Now,
}

impl Undoing<'_> {
    fn note_change(&self, note_id: i64) -> Result<UndoOutcome> {
        let store = NoteStore::new(self.conn);
        let change = self.change;
        self.check_is_last_change_of_note(note_id, change.id)?;
        let mut outcome = self.outcome(format!("{} of note #{note_id}", change.action));

        match change.action {
            Action::Add => {
                let note = existing(&store, note_id)?;
                if let Some(&replaced) = note.replaces.first() {
                    let replace = standing_changes_of_note(self.conn, replaced)?
                        .into_iter()
                        .rfind(|change| change.action == Action::Replace)
                        .ok_or_else(|| anyhow!("note #{note_id} replaced #{replaced}"))?;
                    bail!(
                        "note #{note_id} replaced #{replaced}: undo change #{} instead",
                        replace.id
                    );
                }
                self.remove_note(&store, &note)?;
            }
            Action::Delete => {
                let before = self.note_before()?;
                let note = self.without_missing_people(before, &mut outcome.warnings)?;
                store.put_back(&note)?;
                change_log::record(self.conn, self.now, self.actor, Action::Undo, note_id, None)?;
                outcome.text_changed = true;
            }
            Action::Replace => {
                let old = existing(&store, note_id)?;
                let new_id = old
                    .replaced_by
                    .ok_or_else(|| anyhow!("note #{note_id} is not replaced any more"))?;
                // The new note goes away: it must still be as the replace saved it.
                let changes_of_new = standing_changes_of_note(self.conn, new_id)?;
                match changes_of_new.last() {
                    Some(last) if last.action == Action::Add => {
                        change_log::mark_undone(self.conn, last.id, self.now)?;
                    }
                    Some(last) => bail!(
                        "note #{new_id} changed after change #{}: undo change #{} first",
                        change.id,
                        last.id
                    ),
                    None => bail!("note #{new_id} has no change to undo"),
                }
                let new = existing(&store, new_id)?;
                self.remove_note(&store, &new)?;
                self.put_note_state_back(&store, note_id, &mut outcome)?;
                outcome.text_changed = false;
            }
            Action::Link => {
                let before = self.note_before()?;
                let current = existing(&store, note_id)?;
                let linked: Vec<i64> = current
                    .related
                    .iter()
                    .copied()
                    .filter(|other| !before.related.contains(other))
                    .collect();
                let &[other_id] = linked.as_slice() else {
                    bail!(
                        "change #{} did not link note #{note_id} to one note",
                        change.id
                    );
                };
                // The link is a change of both notes: the other half is undone with it.
                match standing_changes_of_note(self.conn, other_id)?.last() {
                    Some(last) if last.action == Action::Link => {
                        change_log::mark_undone(self.conn, last.id, self.now)?;
                    }
                    Some(last) => bail!(
                        "note #{other_id} changed after change #{}: undo change #{} first",
                        change.id,
                        last.id
                    ),
                    None => bail!("note #{other_id} has no change to undo"),
                }
                self.put_note_state_back(&store, note_id, &mut outcome)?;
                outcome.description = format!(
                    "link of note #{} and note #{}",
                    note_id.min(other_id),
                    note_id.max(other_id)
                );
            }
            Action::Edit | Action::Done | Action::Drop | Action::Postpone => {
                self.put_note_state_back(&store, note_id, &mut outcome)?;
            }
            other => bail!("change #{} ({other}) cannot be undone", change.id),
        }
        Ok(outcome)
    }

    fn person_change(&self, person_id: i64) -> Result<UndoOutcome> {
        let people = PeopleStore::new(self.conn);
        let change = self.change;
        let outcome = self.outcome(format!("{} of person #{person_id}", change.action));
        let standing: Vec<Change> = change_log::changes_of_person(self.conn, person_id)?
            .into_iter()
            .filter(Change::stands)
            .collect();
        self.check_is_last(&standing, "person", person_id, change.id)?;
        let person_now = people
            .get(person_id)?
            .ok_or_else(|| anyhow!("person #{person_id} not found"))?;
        let person_now_json = Some(serde_json::to_string(&person_now)?);

        match change.action {
            Action::PersonAdd => people.remove_unused(person_id)?,
            Action::PersonEdit | Action::Alias => {
                let json = change
                    .before_json
                    .as_deref()
                    .ok_or_else(|| anyhow!("change #{} has no state before", change.id))?;
                people.put_state(&serde_json::from_str::<Person>(json)?)?;
            }
            Action::Merge => {
                people.undo_merge_unlogged(change)?;
            }
            other => bail!("change #{} ({other}) cannot be undone", change.id),
        }
        change_log::record_for_person(
            self.conn,
            self.now,
            self.actor,
            Action::Undo,
            person_id,
            person_now_json,
        )?;
        Ok(outcome)
    }

    fn outcome(&self, description: String) -> UndoOutcome {
        UndoOutcome {
            change: self.change.clone(),
            description,
            warnings: Vec::new(),
            text_changed: false,
        }
    }

    fn note_before(&self) -> Result<Note> {
        self.change
            .note_before()?
            .ok_or_else(|| anyhow!("change #{} has no state before", self.change.id))
    }

    /// Writes the note as it was before the change. The undo entry keeps how it is now.
    fn put_note_state_back(
        &self,
        store: &NoteStore<'_>,
        note_id: i64,
        outcome: &mut UndoOutcome,
    ) -> Result<()> {
        let current = existing(store, note_id)?;
        let before = self.without_missing_people(self.note_before()?, &mut outcome.warnings)?;
        outcome.text_changed = before.title != current.title || before.body != current.body;
        store.apply(note_id, Action::Undo, self.actor, self.now, |_| Ok(before))?;
        Ok(())
    }

    /// Removes a note; the undo entry keeps it, so nothing is lost.
    fn remove_note(&self, store: &NoteStore<'_>, note: &Note) -> Result<()> {
        store.remove(note.id)?;
        change_log::record(
            self.conn,
            self.now,
            self.actor,
            Action::Undo,
            note.id,
            Some(note),
        )?;
        Ok(())
    }

    /// People merged away since the change cannot come back with the note.
    fn without_missing_people(&self, mut note: Note, warnings: &mut Vec<String>) -> Result<Note> {
        let mut known = Vec::new();
        for &person in &note.people {
            if person_name(self.conn, person)?.is_some() {
                known.push(person);
            } else {
                warnings.push(format!(
                    "person #{person} no longer exists: left out of note #{}",
                    note.id
                ));
            }
        }
        note.people = known;
        Ok(note)
    }

    fn check_is_last_change_of_note(&self, note_id: i64, change_id: i64) -> Result<()> {
        let standing = standing_changes_of_note(self.conn, note_id)?;
        self.check_is_last(&standing, "note", note_id, change_id)
    }

    /// Undoing an older change would silently throw away the newer ones.
    fn check_is_last(
        &self,
        standing: &[Change],
        subject: &str,
        subject_id: i64,
        change_id: i64,
    ) -> Result<()> {
        match standing.last() {
            Some(last) if last.id != change_id => bail!(
                "{subject} #{subject_id} changed after change #{change_id}: \
                 undo change #{} first",
                last.id
            ),
            _ => Ok(()),
        }
    }
}

fn existing(store: &NoteStore<'_>, note_id: i64) -> Result<Note> {
    store
        .get(note_id)?
        .ok_or_else(|| anyhow!("note #{note_id} not found"))
}

/// The changes of a note that can still be undone, oldest first.
fn standing_changes_of_note(conn: &Connection, note_id: i64) -> Result<Vec<Change>> {
    Ok(change_log::changes_of_note(conn, note_id)?
        .into_iter()
        .filter(Change::stands)
        .collect())
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;

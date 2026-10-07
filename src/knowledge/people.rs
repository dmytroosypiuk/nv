//! Saves and finds people. Every change goes to the change log.

use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::change_log::{self, Action, Actor};
use super::person::{Person, PersonError, is_full_alias, same_name, tidy};
use crate::clock::Now;

/// The main name of a person, or `None` when there is no person with this ID.
pub fn person_name(conn: &Connection, person_id: i64) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT name FROM people WHERE id = ?1",
            [person_id],
            |row| row.get(0),
        )
        .optional()?)
}

/// What a merge changed, kept in the change log so that it can be undone.
#[derive(Debug, Serialize, Deserialize)]
struct MergeBefore {
    kept: Person,
    merged: Person,
    /// Notes that were linked to the merged person only.
    moved_note_ids: Vec<i64>,
    /// Notes that were linked to both people.
    shared_note_ids: Vec<i64>,
    /// Commitments the merged person owned.
    owned_note_ids: Vec<i64>,
}

pub struct PeopleStore<'c> {
    conn: &'c Connection,
}

impl<'c> PeopleStore<'c> {
    pub fn new(conn: &'c Connection) -> Self {
        Self { conn }
    }

    pub fn add(
        &self,
        name: &str,
        role: Option<&str>,
        aliases: &[String],
        actor: Actor,
        now: &Now,
    ) -> Result<Person> {
        let everyone = self.everyone()?;
        let mut person = Person {
            id: 0,
            name: tidy(name),
            role: role.map(tidy).filter(|role| !role.is_empty()),
            aliases: Vec::new(),
        };
        if person.name.is_empty() {
            return Err(PersonError::MissingName.into());
        }
        check_name_is_free(&everyone, &person.name, person.id)?;
        for alias in aliases {
            let alias = tidy(alias);
            if !alias.is_empty() && !has_name(&person, &alias) {
                check_alias_is_free(&everyone, &alias, person.id)?;
                person.aliases.push(alias);
            }
        }

        let tx = self.conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO people (name, role) VALUES (?1, ?2)",
            params![person.name, person.role],
        )?;
        person.id = tx.last_insert_rowid();
        save(&tx, &person)?;
        change_log::record_for_person(&tx, now, actor, Action::PersonAdd, person.id, None)?;
        tx.commit()?;
        Ok(person)
    }

    pub fn get(&self, id: i64) -> Result<Option<Person>> {
        Ok(self.everyone()?.into_iter().find(|person| person.id == id))
    }

    /// Changes the main name, the role or both. The old main name stays as an alias.
    pub fn edit(
        &self,
        id: i64,
        name: Option<&str>,
        role: Option<&str>,
        actor: Actor,
        now: &Now,
    ) -> Result<Person> {
        if name.is_none() && role.is_none() {
            return Err(PersonError::NothingToChange.into());
        }
        let everyone = self.everyone()?;
        let before = existing(&everyone, id)?;
        let mut after = before.clone();
        if let Some(name) = name {
            let name = tidy(name);
            if name.is_empty() {
                return Err(PersonError::MissingName.into());
            }
            check_name_is_free(&everyone, &name, id)?;
            after.aliases.retain(|alias| !same_name(alias, &name));
            if !same_name(&before.name, &name) {
                after.aliases.push(before.name.clone());
            }
            after.name = name;
        }
        if let Some(role) = role {
            after.role = Some(tidy(role)).filter(|role| !role.is_empty());
        }
        self.save_change(&before, &after, Action::PersonEdit, actor, now)?;
        Ok(after)
    }

    /// Everyone, by main name.
    pub fn list(&self) -> Result<Vec<Person>> {
        let mut everyone = self.everyone()?;
        everyone.sort_by_key(|person| person.name.to_lowercase());
        Ok(everyone)
    }

    /// Everyone whose main name or an alias contains `text`, without case.
    pub fn search(&self, text: &str) -> Result<Vec<Person>> {
        let text = tidy(text).to_lowercase();
        let mut found = self.list()?;
        found.retain(|person| {
            person.name.to_lowercase().contains(&text)
                || person
                    .aliases
                    .iter()
                    .any(|alias| alias.to_lowercase().contains(&text))
        });
        Ok(found)
    }

    pub fn add_alias(&self, id: i64, alias: &str, actor: Actor, now: &Now) -> Result<Person> {
        let alias = tidy(alias);
        if alias.is_empty() {
            return Err(PersonError::BlankAlias.into());
        }
        let everyone = self.everyone()?;
        let before = existing(&everyone, id)?;
        if has_name(&before, &alias) {
            // Nothing new: nothing to save, nothing to log.
            return Ok(before);
        }
        check_alias_is_free(&everyone, &alias, id)?;
        let mut after = before.clone();
        after.aliases.push(alias);
        self.save_change(&before, &after, Action::Alias, actor, now)?;
        Ok(after)
    }

    /// Makes one person out of two: notes, owned commitments and aliases of `other_id`
    /// move to `keep_id`, the other main name becomes an alias, the other person is removed.
    pub fn merge(&self, keep_id: i64, other_id: i64, actor: Actor, now: &Now) -> Result<Person> {
        if keep_id == other_id {
            return Err(PersonError::SamePerson.into());
        }
        let everyone = self.everyone()?;
        let kept = existing(&everyone, keep_id)?;
        let merged = existing(&everyone, other_id)?;

        let mut after = kept.clone();
        for name in std::iter::once(&merged.name).chain(&merged.aliases) {
            if !has_name(&after, name) {
                after.aliases.push(name.clone());
            }
        }
        // The role of the kept person wins.
        after.role = kept.role.clone().or_else(|| merged.role.clone());

        let linked_to_kept = self.note_ids(
            "SELECT note_id FROM note_people WHERE person_id = ?1",
            keep_id,
        )?;
        let (shared_note_ids, moved_note_ids): (Vec<i64>, Vec<i64>) = self
            .note_ids(
                "SELECT note_id FROM note_people WHERE person_id = ?1",
                other_id,
            )?
            .into_iter()
            .partition(|note_id| linked_to_kept.contains(note_id));
        let before = MergeBefore {
            kept,
            merged,
            moved_note_ids,
            shared_note_ids,
            owned_note_ids: self
                .note_ids("SELECT id FROM notes WHERE owner_person_id = ?1", other_id)?,
        };

        let tx = self.conn.unchecked_transaction()?;
        // Notes linked to both people keep one link; the rest move.
        for note_id in &before.shared_note_ids {
            tx.execute(
                "DELETE FROM note_people WHERE note_id = ?1 AND person_id = ?2",
                params![note_id, other_id],
            )?;
        }
        tx.execute(
            "UPDATE note_people SET person_id = ?1 WHERE person_id = ?2",
            params![keep_id, other_id],
        )?;
        tx.execute(
            "UPDATE notes SET owner_person_id = ?1 WHERE owner_person_id = ?2",
            params![keep_id, other_id],
        )?;
        tx.execute("DELETE FROM people WHERE id = ?1", [other_id])?;
        save(&tx, &after)?;
        let before_json = serde_json::to_string(&before)?;
        change_log::record_for_person(&tx, now, actor, Action::Merge, keep_id, Some(before_json))?;
        tx.commit()?;
        Ok(after)
    }

    /// Takes a merge back: the removed person returns with their ID, aliases, notes and
    /// commitments. Returns the two people as they are again.
    pub fn undo_merge(&self, change_id: i64, actor: Actor, now: &Now) -> Result<(Person, Person)> {
        let change = change_log::change(self.conn, change_id)?
            .ok_or_else(|| anyhow!("change #{change_id} not found"))?;
        let before: MergeBefore = match (change.action, &change.before_json) {
            (Action::Merge, Some(json)) => serde_json::from_str(json)
                .with_context(|| format!("change #{change_id} holds a merge nv cannot read"))?,
            _ => bail!("change #{change_id} is not a merge"),
        };
        let MergeBefore {
            kept,
            merged,
            moved_note_ids,
            shared_note_ids,
            owned_note_ids,
        } = before;
        let everyone = self.everyone()?;
        existing(&everyone, kept.id)?;
        if everyone.iter().any(|person| person.id == merged.id) {
            bail!("person #{} already exists", merged.id);
        }

        let tx = self.conn.unchecked_transaction()?;
        // The kept person first: it gives back the names the returning person needs.
        save(&tx, &kept)?;
        tx.execute(
            "INSERT INTO people (id, name, role) VALUES (?1, ?2, ?3)",
            params![merged.id, merged.name, merged.role],
        )?;
        save(&tx, &merged)?;
        // Notes deleted since the merge are simply not there to update.
        for note_id in moved_note_ids {
            tx.execute(
                "UPDATE note_people SET person_id = ?1 WHERE note_id = ?2 AND person_id = ?3",
                params![merged.id, note_id, kept.id],
            )?;
        }
        for note_id in shared_note_ids {
            tx.execute(
                "INSERT OR IGNORE INTO note_people (note_id, person_id)
                 SELECT id, ?1 FROM notes WHERE id = ?2",
                params![merged.id, note_id],
            )?;
        }
        for note_id in owned_note_ids {
            tx.execute(
                "UPDATE notes SET owner_person_id = ?1 WHERE id = ?2 AND owner_person_id = ?3",
                params![merged.id, note_id, kept.id],
            )?;
        }
        change_log::record_for_person(&tx, now, actor, Action::Undo, kept.id, None)?;
        tx.commit()?;
        Ok((kept, merged))
    }

    fn note_ids(&self, sql: &str, person_id: i64) -> Result<Vec<i64>> {
        let mut statement = self.conn.prepare(sql)?;
        let ids = statement.query_map([person_id], |row| row.get(0))?;
        Ok(ids.collect::<rusqlite::Result<_>>()?)
    }

    fn save_change(
        &self,
        before: &Person,
        after: &Person,
        action: Action,
        actor: Actor,
        now: &Now,
    ) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        save(&tx, after)?;
        let before_json = serde_json::to_string(before)?;
        change_log::record_for_person(&tx, now, actor, action, after.id, Some(before_json))?;
        tx.commit()?;
        Ok(())
    }

    /// All people with their aliases, by ID. There are few people: rules are checked in
    /// Rust, where names compare without case in any language.
    fn everyone(&self) -> Result<Vec<Person>> {
        let mut statement = self
            .conn
            .prepare("SELECT id, name, role FROM people ORDER BY id")?;
        let people = statement.query_map([], |row| {
            Ok(Person {
                id: row.get(0)?,
                name: row.get(1)?,
                role: row.get(2)?,
                aliases: Vec::new(),
            })
        })?;
        let mut everyone = people.collect::<rusqlite::Result<Vec<_>>>()?;
        let mut statement = self
            .conn
            .prepare("SELECT person_id, alias FROM person_aliases ORDER BY rowid")?;
        let aliases = statement.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for alias in aliases {
            let (person_id, alias) = alias?;
            if let Some(person) = everyone.iter_mut().find(|person| person.id == person_id) {
                person.aliases.push(alias);
            }
        }
        Ok(everyone)
    }
}

fn existing(everyone: &[Person], id: i64) -> Result<Person> {
    everyone
        .iter()
        .find(|person| person.id == id)
        .cloned()
        .ok_or_else(|| PersonError::NotFound { id }.into())
}

/// True when `name` is the person's main name or one of their aliases.
fn has_name(person: &Person, name: &str) -> bool {
    same_name(&person.name, name) || person.aliases.iter().any(|alias| same_name(alias, name))
}

/// A main name belongs to one person; it also cannot be someone else's full alias.
fn check_name_is_free(everyone: &[Person], name: &str, person_id: i64) -> Result<()> {
    let taken_by = everyone.iter().find(|other| {
        other.id != person_id
            && (same_name(&other.name, name) || (is_full_alias(name) && has_name(other, name)))
    });
    match taken_by {
        Some(other) => Err(PersonError::NameTaken {
            name: name.to_string(),
            id: other.id,
            owner: other.name.clone(),
        }
        .into()),
        None => Ok(()),
    }
}

/// A full alias belongs to one person. A short alias can match several people.
fn check_alias_is_free(everyone: &[Person], alias: &str, person_id: i64) -> Result<()> {
    if !is_full_alias(alias) {
        return Ok(());
    }
    match everyone
        .iter()
        .find(|other| other.id != person_id && has_name(other, alias))
    {
        Some(other) => Err(PersonError::AliasTaken {
            alias: alias.to_string(),
            id: other.id,
            owner: other.name.clone(),
        }
        .into()),
        None => Ok(()),
    }
}

/// Writes name, role and aliases of an existing person.
fn save(conn: &Connection, person: &Person) -> Result<()> {
    conn.execute(
        "UPDATE people SET name = ?2, role = ?3 WHERE id = ?1",
        params![person.id, person.name, person.role],
    )?;
    conn.execute(
        "DELETE FROM person_aliases WHERE person_id = ?1",
        [person.id],
    )?;
    for alias in &person.aliases {
        conn.execute(
            "INSERT INTO person_aliases (person_id, alias) VALUES (?1, ?2)",
            params![person.id, alias],
        )?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "people_tests.rs"]
mod tests;

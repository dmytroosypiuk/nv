//! Saves and finds people. Every change goes to the change log.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

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

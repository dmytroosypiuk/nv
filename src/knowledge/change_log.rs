//! The change log: every add, edit and delete, with the state before it.

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::note::{Note, word_enum};
use crate::clock::Now;

word_enum!(
    /// Who made a change.
    Actor, "actor", { Claude => "claude", User => "user" }
);
word_enum!(
    Action, "action", {
        Add => "add",
        Edit => "edit",
        Delete => "delete",
        Restore => "restore",
        Done => "done",
        Drop => "drop",
        Postpone => "postpone",
        Replace => "replace",
        Link => "link",
        PersonAdd => "person-add",
        PersonEdit => "person-edit",
        Alias => "alias",
        Merge => "merge",
        Undo => "undo",
    }
);

impl Actor {
    /// `NV_ACTOR` decides when set; otherwise commands run by Claude Code are Claude's.
    pub fn from_env(nv_actor: Option<&str>, run_by_claude_code: bool) -> Result<Self> {
        match nv_actor {
            Some(word) if !word.is_empty() => Ok(word.parse()?),
            _ if run_by_claude_code => Ok(Self::Claude),
            _ => Ok(Self::User),
        }
    }
}

/// One entry of the change log. It is about a note or about a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: i64,
    pub at: String,
    pub actor: Actor,
    pub action: Action,
    pub note_id: Option<i64>,
    pub person_id: Option<i64>,
    /// The state before the change, as JSON; `None` when there was nothing before.
    pub before_json: Option<String>,
    /// When the change was undone; `None` when it still stands.
    pub undone_at: Option<String>,
}

impl Change {
    /// The note as it was before the change; `None` when it did not exist.
    pub fn note_before(&self) -> Result<Option<Note>> {
        self.before_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .with_context(|| format!("change #{} holds a note nv cannot read", self.id))
    }
}

/// Logs a change of a note, with the note as it was before.
pub(super) fn record(
    conn: &Connection,
    now: &Now,
    actor: Actor,
    action: Action,
    note_id: i64,
    before: Option<&Note>,
) -> Result<i64> {
    let before_json = before.map(serde_json::to_string).transpose()?;
    insert(conn, now, actor, action, Some(note_id), None, before_json)
}

/// Logs a change of a person, with what undo needs as JSON.
pub(super) fn record_for_person(
    conn: &Connection,
    now: &Now,
    actor: Actor,
    action: Action,
    person_id: i64,
    before_json: Option<String>,
) -> Result<i64> {
    insert(conn, now, actor, action, None, Some(person_id), before_json)
}

fn insert(
    conn: &Connection,
    now: &Now,
    actor: Actor,
    action: Action,
    note_id: Option<i64>,
    person_id: Option<i64>,
    before_json: Option<String>,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO change_log (at, actor, action, note_id, person_id, before_json)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        rusqlite::params![
            now.timestamp(),
            actor.as_str(),
            action.as_str(),
            note_id,
            person_id,
            before_json
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

const COLUMNS: &str = "id, at, actor, action, note_id, person_id, before_json, undone_at";

/// All changes of one note, oldest first.
pub fn changes_of_note(conn: &Connection, note_id: i64) -> Result<Vec<Change>> {
    changes_where(conn, "note_id = ?1", note_id)
}

/// All changes of one person, oldest first.
pub fn changes_of_person(conn: &Connection, person_id: i64) -> Result<Vec<Change>> {
    changes_where(conn, "person_id = ?1", person_id)
}

/// One change by its ID.
pub fn change(conn: &Connection, change_id: i64) -> Result<Option<Change>> {
    Ok(changes_where(conn, "id = ?1", change_id)?.pop())
}

/// The change log, newest first; with a note or a person, only their changes.
pub fn recent(
    conn: &Connection,
    note_id: Option<i64>,
    person_id: Option<i64>,
    limit: usize,
) -> Result<Vec<Change>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM change_log
         WHERE (?1 IS NULL OR note_id = ?1) AND (?2 IS NULL OR person_id = ?2)
         ORDER BY id DESC LIMIT ?3"
    ))?;
    let rows = statement.query_map(rusqlite::params![note_id, person_id, limit as i64], raw)?;
    rows.map(|row| from_raw(row?)).collect()
}

impl Change {
    /// A change that can still be undone: it was not undone and is not itself an undo.
    pub fn stands(&self) -> bool {
        self.undone_at.is_none() && !matches!(self.action, Action::Undo | Action::Restore)
    }
}

/// Marks a change as undone. It stays in the log.
pub(super) fn mark_undone(conn: &Connection, change_id: i64, now: &Now) -> Result<()> {
    conn.execute(
        "UPDATE change_log SET undone_at = ?2 WHERE id = ?1",
        rusqlite::params![change_id, now.timestamp()],
    )?;
    Ok(())
}

fn changes_where(conn: &Connection, condition: &str, id: i64) -> Result<Vec<Change>> {
    let mut statement = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM change_log WHERE {condition} ORDER BY id"
    ))?;
    let rows = statement.query_map([id], raw)?;
    rows.map(|row| from_raw(row?)).collect()
}

type RawChange = (
    i64,
    String,
    String,
    String,
    Option<i64>,
    Option<i64>,
    Option<String>,
    Option<String>,
);

fn raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawChange> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
    ))
}

fn from_raw(raw: RawChange) -> Result<Change> {
    let (id, at, actor, action, note_id, person_id, before_json, undone_at) = raw;
    Ok(Change {
        id,
        at,
        actor: actor.parse()?,
        action: action.parse()?,
        note_id,
        person_id,
        before_json,
        undone_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_is_user_unless_claude_code_runs_the_command() {
        assert_eq!(Actor::from_env(None, false).unwrap(), Actor::User);
        assert_eq!(Actor::from_env(None, true).unwrap(), Actor::Claude);
        assert_eq!(Actor::from_env(Some(""), true).unwrap(), Actor::Claude);
    }

    #[test]
    fn nv_actor_env_var_wins() {
        assert_eq!(Actor::from_env(Some("user"), true).unwrap(), Actor::User);
        assert_eq!(
            Actor::from_env(Some("claude"), false).unwrap(),
            Actor::Claude
        );
        assert!(Actor::from_env(Some("robot"), false).is_err());
    }
}

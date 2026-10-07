//! The change log: every add, edit and delete, with the state before it.

use anyhow::{Context, Result};
use rusqlite::{Connection, Row};

use super::note::{Note, word_enum};
use crate::clock::Now;

word_enum!(
    /// Who made a change.
    Actor, "actor", { Claude => "claude", User => "user" }
);
word_enum!(
    Action, "action", { Add => "add", Edit => "edit", Delete => "delete", Restore => "restore" }
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

/// One entry of the change log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub id: i64,
    pub at: String,
    pub actor: Actor,
    pub action: Action,
    pub note_id: i64,
    /// The note as it was before the change; `None` when it did not exist.
    pub before: Option<Note>,
}

pub(super) fn record(
    conn: &Connection,
    now: &Now,
    actor: Actor,
    action: Action,
    note_id: i64,
    before: Option<&Note>,
) -> Result<i64> {
    let before_json = before.map(serde_json::to_string).transpose()?;
    conn.execute(
        "INSERT INTO change_log (at, actor, action, note_id, before_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            now.timestamp(),
            actor.as_str(),
            action.as_str(),
            note_id,
            before_json
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

/// All changes of one note, oldest first.
pub fn changes_of_note(conn: &Connection, note_id: i64) -> Result<Vec<Change>> {
    let mut statement = conn.prepare(
        "SELECT id, at, actor, action, note_id, before_json
         FROM change_log WHERE note_id = ?1 ORDER BY id",
    )?;
    let rows = statement.query_map([note_id], raw_change)?;
    rows.map(|row| change_from_raw(row?)).collect()
}

/// One change by its ID.
pub fn change(conn: &Connection, change_id: i64) -> Result<Option<Change>> {
    let mut statement = conn.prepare(
        "SELECT id, at, actor, action, note_id, before_json FROM change_log WHERE id = ?1",
    )?;
    let mut rows = statement.query_map([change_id], raw_change)?;
    rows.next().map(|row| change_from_raw(row?)).transpose()
}

type RawChange = (i64, String, String, String, i64, Option<String>);

fn raw_change(row: &Row<'_>) -> rusqlite::Result<RawChange> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}

fn change_from_raw((id, at, actor, action, note_id, before_json): RawChange) -> Result<Change> {
    let before = before_json
        .map(|json| serde_json::from_str(&json))
        .transpose()
        .with_context(|| format!("change #{id} holds a note nv cannot read"))?;
    Ok(Change {
        id,
        at,
        actor: actor.parse()?,
        action: action.parse()?,
        note_id,
        before,
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

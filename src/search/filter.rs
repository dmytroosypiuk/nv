//! Which notes a search may return: filters and the expiry rule.

use anyhow::Result;
use rusqlite::{Connection, params};

use crate::clock::Date;
use crate::knowledge::note::{Area, NoteType};

/// All filters combine with AND.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteFilter {
    pub area: Option<Area>,
    pub note_type: Option<NoteType>,
    pub repo: Option<String>,
    pub ticket: Option<String>,
    /// Notes created on or after this day.
    pub since: Option<Date>,
    /// Commitments planned for this day.
    pub planned: Option<Date>,
    /// `--all`: expired notes too.
    pub include_expired: bool,
}

/// A note that passed the filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    pub note_id: i64,
    pub outdated: bool,
}

impl NoteFilter {
    /// True when at least one filter narrows the search (`--all` only widens it).
    pub fn narrows(&self) -> bool {
        self.area.is_some()
            || self.note_type.is_some()
            || self.repo.is_some()
            || self.ticket.is_some()
            || self.since.is_some()
            || self.planned.is_some()
    }

    /// The notes that pass, newest first. A note is expired when `today` is after its
    /// `expires_on`.
    pub fn candidates(&self, conn: &Connection, today: Date) -> Result<Vec<Candidate>> {
        // A filter that is not given is NULL and lets every note pass.
        let mut statement = conn.prepare(
            "SELECT id, status = 'outdated' FROM notes
             WHERE (?1 IS NULL OR area = ?1)
               AND (?2 IS NULL OR type = ?2)
               AND (?3 IS NULL OR EXISTS
                     (SELECT 1 FROM note_repos WHERE note_id = notes.id AND repo = ?3))
               AND (?4 IS NULL OR EXISTS
                     (SELECT 1 FROM note_tickets WHERE note_id = notes.id AND ticket_id = ?4))
               AND (?5 IS NULL OR substr(created_at, 1, 10) >= ?5)
               AND (?6 IS NULL OR planned_for = ?6)
               AND (?7 OR expires_on IS NULL OR expires_on >= ?8)
             ORDER BY created_at DESC, id DESC",
        )?;
        let candidates = statement.query_map(
            params![
                self.area.map(|area| area.as_str()),
                self.note_type.map(|note_type| note_type.as_str()),
                self.repo,
                self.ticket,
                self.since.map(|date| date.to_string()),
                self.planned.map(|date| date.to_string()),
                self.include_expired,
                today.to_string(),
            ],
            |row| {
                Ok(Candidate {
                    note_id: row.get(0)?,
                    outdated: row.get(1)?,
                })
            },
        )?;
        Ok(candidates.collect::<rusqlite::Result<_>>()?)
    }
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;

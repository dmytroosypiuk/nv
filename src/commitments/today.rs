//! The today view: what I planned for today, and what others owe me.

use anyhow::Result;
use rusqlite::Connection;
use serde::Serialize;

use crate::clock::Date;
use crate::knowledge::note::Note;
use crate::knowledge::people::person_name;
use crate::knowledge::store::NoteStore;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TodayView {
    pub today: Date,
    /// My open commitments planned for today or earlier, oldest plan first.
    pub mine: Vec<Note>,
    /// Open commitments of other people, soonest first, undated last.
    pub owed: Vec<OwedCommitment>,
    /// How many of my open commitments have no planned date.
    pub undated: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OwedCommitment {
    pub owner_name: String,
    #[serde(flatten)]
    pub note: Note,
}

impl TodayView {
    pub fn is_empty(&self) -> bool {
        self.mine.is_empty() && self.owed.is_empty() && self.undated == 0
    }
}

/// Only open (`todo`), active, not expired commitments are shown.
pub fn today_view(conn: &Connection, today: Date) -> Result<TodayView> {
    let store = NoteStore::new(conn);
    let today_text = today.to_string();
    let load = |ids: Vec<i64>| -> Result<Vec<Note>> {
        let mut notes = Vec::new();
        for id in ids {
            notes.extend(store.get(id)?);
        }
        Ok(notes)
    };

    let mine = load(open_commitment_ids(
        conn,
        "owner_person_id IS NULL AND planned_for <= ?1",
        "planned_for, id",
        &today_text,
    )?)?;
    let undated = open_commitment_ids(
        conn,
        "owner_person_id IS NULL AND planned_for IS NULL",
        "id",
        &today_text,
    )?
    .len();
    let mut owed = Vec::new();
    for note in load(open_commitment_ids(
        conn,
        "owner_person_id IS NOT NULL",
        "planned_for IS NULL, planned_for, id",
        &today_text,
    )?)? {
        let owner_name = match note.owner {
            Some(owner) => person_name(conn, owner)?,
            None => None,
        };
        owed.push(OwedCommitment {
            owner_name: owner_name.unwrap_or_else(|| "someone".to_string()),
            note,
        });
    }

    Ok(TodayView {
        today,
        mine,
        owed,
        undated,
    })
}

/// IDs of `todo` commitments that are active and not expired on `today`, narrowed by
/// `condition`, which may use `?1` for today.
fn open_commitment_ids(
    conn: &Connection,
    condition: &str,
    order: &str,
    today: &str,
) -> Result<Vec<i64>> {
    let mut statement = conn.prepare(&format!(
        "SELECT id FROM notes
         WHERE type = 'commitment' AND commitment_status = 'todo' AND status = 'active'
           AND (expires_on IS NULL OR expires_on >= ?1)
           AND {condition}
         ORDER BY {order}"
    ))?;
    let ids = statement.query_map([today], |row| row.get(0))?;
    Ok(ids.collect::<rusqlite::Result<_>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commitments::{mark_done, mark_dropped};
    use crate::db;
    use crate::knowledge::change_log::{Action, Actor};
    use crate::knowledge::note::NoteType;
    use crate::test_support::{MONDAY, add_at, add_note, at, work_note};

    const TODAY: &str = "2026-10-07";
    const ANNA: i64 = 7;
    const PIOTR: i64 = 9;

    fn date(text: &str) -> Date {
        text.parse().unwrap()
    }

    fn database() -> Connection {
        let conn = db::open_in_memory().unwrap();
        // People commands come in step 5.
        conn.execute_batch(
            "INSERT INTO people (id, name) VALUES (7, 'Anna Nowak'), (9, 'Piotr Zielinski');",
        )
        .unwrap();
        conn
    }

    fn commit(conn: &Connection, title: &str, owner: Option<i64>, planned: Option<&str>) -> i64 {
        let mut fields = work_note(title, "Promised on the daily.");
        fields.note_type = Some(NoteType::Commitment);
        fields.owner = owner;
        fields.planned_for = planned.map(date);
        add_at(conn, fields, MONDAY)
    }

    fn view(conn: &Connection) -> TodayView {
        today_view(conn, date(TODAY)).unwrap()
    }

    fn ids(notes: &[Note]) -> Vec<i64> {
        notes.iter().map(|note| note.id).collect()
    }

    fn owed_ids(view: &TodayView) -> Vec<i64> {
        view.owed.iter().map(|owed| owed.note.id).collect()
    }

    #[test]
    fn today_lists_my_planned_commitments_and_what_others_owe_me() {
        let conn = database();
        let mine = commit(&conn, "Send retry numbers to Anna", None, Some(TODAY));
        let annas = commit(&conn, "Review the retry PR", Some(ANNA), Some("2026-10-08"));
        add_note(&conn, "Retry 5 times", "Not a commitment.");

        let view = view(&conn);

        assert_eq!(view.today, date(TODAY));
        assert_eq!(ids(&view.mine), [mine]);
        assert_eq!(owed_ids(&view), [annas]);
        assert_eq!(view.owed[0].owner_name, "Anna Nowak");
        assert_eq!(view.undated, 0);
        assert!(!view.is_empty());
    }

    #[test]
    fn today_includes_my_overdue_commitments() {
        let conn = database();
        let today = commit(&conn, "Send retry numbers", None, Some(TODAY));
        let overdue = commit(&conn, "Book the exam slot", None, Some("2026-10-05"));

        assert_eq!(ids(&view(&conn).mine), [overdue, today]);
    }

    #[test]
    fn today_skips_done_and_dropped() {
        let conn = database();
        let store = NoteStore::new(&conn);
        let done = commit(&conn, "Send retry numbers", None, Some(TODAY));
        let dropped = commit(&conn, "Book the exam slot", None, Some(TODAY));
        let annas_done = commit(&conn, "Review the retry PR", Some(ANNA), None);
        let now = at("2026-10-07T08:00:00+02:00");
        for id in [done, annas_done] {
            store
                .change(id, Action::Done, Actor::User, &now, |note| {
                    Ok(mark_done(note, &now)?)
                })
                .unwrap();
        }
        store
            .change(dropped, Action::Drop, Actor::User, &now, |note| {
                Ok(mark_dropped(note, &now)?)
            })
            .unwrap();

        assert!(view(&conn).is_empty());
    }

    #[test]
    fn today_skips_my_commitments_planned_for_later() {
        let conn = database();
        commit(&conn, "Send retry numbers", None, Some("2026-10-08"));

        let view = view(&conn);

        assert!(view.mine.is_empty());
        assert_eq!(view.undated, 0);
    }

    #[test]
    fn today_counts_my_commitments_without_a_date() {
        let conn = database();
        commit(&conn, "Read the SQLite book", None, None);
        commit(&conn, "Clean the backlog", None, None);
        commit(&conn, "Send the staging steps", Some(PIOTR), None);

        let view = view(&conn);

        assert!(view.mine.is_empty());
        assert_eq!(view.undated, 2);
        assert!(!view.is_empty());
    }

    #[test]
    fn today_skips_outdated_and_expired_commitments() {
        let conn = database();
        let outdated = commit(&conn, "Send retry numbers", None, Some(TODAY));
        // No command marks a note outdated before step 6.
        conn.execute(
            "UPDATE notes SET status = 'outdated' WHERE id = ?1",
            [outdated],
        )
        .unwrap();
        let mut expired = work_note("Lend Anna the adapter", "Until Monday.");
        expired.note_type = Some(NoteType::Commitment);
        expired.owner = Some(ANNA);
        expired.expires_on = Some(date("2026-10-06"));
        add_at(&conn, expired, MONDAY);

        assert!(view(&conn).is_empty());
    }

    #[test]
    fn others_owe_me_is_sorted_by_planned_date_undated_last() {
        let conn = database();
        let undated = commit(&conn, "Send the staging steps", Some(PIOTR), None);
        let later = commit(&conn, "Review the retry PR", Some(ANNA), Some("2026-10-12"));
        let overdue = commit(&conn, "Send the invoice", Some(PIOTR), Some("2026-10-01"));

        let view = view(&conn);

        assert_eq!(owed_ids(&view), [overdue, later, undated]);
        assert_eq!(view.owed[0].owner_name, "Piotr Zielinski");
    }
}

//! Helpers shared by unit tests.

use rusqlite::Connection;

use crate::clock::Now;
use crate::knowledge::change_log::Actor;
use crate::knowledge::note::{Area, NoteDraft, NoteFields};
use crate::knowledge::store::NoteStore;

pub const MONDAY: &str = "2026-10-05T09:00:00+02:00";

pub fn at(time: &str) -> Now {
    Now::parse(time).unwrap()
}

pub fn work_note(title: &str, body: &str) -> NoteFields {
    NoteFields {
        title: title.into(),
        body: body.into(),
        area: Area::Work,
        note_type: None,
        project: None,
        repos: vec![],
        tickets: vec![],
        people: vec![],
        source: None,
        expires_on: None,
        owner: None,
        planned_for: None,
    }
}

/// Saves the note at the given time and returns its ID.
pub fn add_at(conn: &Connection, fields: NoteFields, time: &str) -> i64 {
    let draft = NoteDraft::new(fields).unwrap();
    NoteStore::new(conn)
        .add(&draft, Actor::Claude, &at(time))
        .unwrap()
        .id
}

pub fn add_note(conn: &Connection, title: &str, body: &str) -> i64 {
    add_at(conn, work_note(title, body), MONDAY)
}

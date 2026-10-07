use super::*;
use crate::db;
use crate::knowledge::change_log::{self, Action};
use crate::knowledge::note::{
    Area, CommitmentStatus, NoteFields, NoteStatus, NoteType, Replacement, Source, SourceKind,
};

const MONDAY: &str = "2026-10-05T09:00:00+02:00";
const TUESDAY: &str = "2026-10-06T10:30:00+02:00";

fn at(time: &str) -> Now {
    Now::parse(time).unwrap()
}

fn retry_fields() -> NoteFields {
    NoteFields {
        title: "Retry 5 times".into(),
        body: "We agreed with Anna to use 5 retries.\nCode: `retry(max = 5)`".into(),
        area: Area::Work,
        note_type: Some(NoteType::Decision),
        project: Some("Billing".into()),
        repos: vec!["billing-api".into(), "gateway".into()],
        tickets: vec!["PAY-1234".into()],
        people: vec![],
        source: Some(Source {
            kind: SourceKind::Meeting,
            reference: "Sprint planning, 2026-10-05".into(),
        }),
        expires_on: None,
        owner: None,
        planned_for: None,
    }
}

fn retry_draft() -> NoteDraft {
    NoteDraft::new(retry_fields()).unwrap()
}

#[test]
fn added_note_can_be_read_back_with_repos_and_tickets() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);

    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();

    let fields = retry_fields();
    assert_eq!(
        added,
        Note {
            id: 1,
            title: fields.title,
            body: fields.body,
            area: Area::Work,
            note_type: Some(NoteType::Decision),
            project: Some("Billing".into()),
            status: NoteStatus::Active,
            commitment_status: None,
            source: fields.source,
            repos: fields.repos,
            tickets: fields.tickets,
            people: vec![],
            expires_on: None,
            owner: None,
            planned_for: None,
            closed_at: None,
            replaced_by: None,
            replaces: vec![],
            related: vec![],
            created_at: MONDAY.into(),
            updated_at: MONDAY.into(),
        }
    );
    assert_eq!(store.get(1).unwrap(), Some(added));
    assert_eq!(store.get(2).unwrap(), None);
}

#[test]
fn new_commitment_is_saved_as_todo() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let mut fields = retry_fields();
    fields.note_type = Some(NoteType::Commitment);

    let added = store
        .add(&NoteDraft::new(fields).unwrap(), Actor::User, &at(MONDAY))
        .unwrap();

    assert_eq!(added.commitment_status, Some(CommitmentStatus::Todo));
    assert_eq!(store.get(added.id).unwrap().unwrap(), added);
}

#[test]
fn add_writes_change_log_without_previous_state() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);

    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();

    let changes = change_log::changes_of_note(&conn, added.id).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].action, Action::Add);
    assert_eq!(changes[0].at, MONDAY);
    assert_eq!(changes[0].note_before().unwrap(), None);
}

#[test]
fn edit_changes_only_given_fields_and_updated_at() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();
    let changes = NoteChanges {
        title: Some("Retry 5 times for billing-api".into()),
        repos: Some(vec!["billing-api".into()]),
        ..NoteChanges::default()
    };

    let edited = store
        .edit(added.id, &changes, Actor::User, &at(TUESDAY))
        .unwrap();

    assert_eq!(
        edited,
        Note {
            title: "Retry 5 times for billing-api".into(),
            repos: vec!["billing-api".into()],
            updated_at: TUESDAY.into(),
            ..added
        }
    );
    assert_eq!(store.get(edited.id).unwrap(), Some(edited));
}

#[test]
fn edit_writes_change_log_with_previous_state() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();
    let changes = NoteChanges {
        body: Some("Now 7 retries.".into()),
        ..NoteChanges::default()
    };

    store
        .edit(added.id, &changes, Actor::User, &at(TUESDAY))
        .unwrap();

    let log = change_log::changes_of_note(&conn, added.id).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].action, Action::Edit);
    assert_eq!(log[1].at, TUESDAY);
    assert_eq!(log[1].note_before().unwrap(), Some(added));
}

#[test]
fn edit_of_missing_note_fails() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let changes = NoteChanges {
        title: Some("New".into()),
        ..NoteChanges::default()
    };

    let error = store
        .edit(99, &changes, Actor::User, &at(MONDAY))
        .unwrap_err();

    assert_eq!(error.to_string(), "note #99 not found");
}

#[test]
fn refused_edit_saves_nothing() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();
    let changes = NoteChanges {
        body: Some("password=hunter2".into()),
        ..NoteChanges::default()
    };

    let error = store
        .edit(added.id, &changes, Actor::User, &at(TUESDAY))
        .unwrap_err();

    assert!(
        error.to_string().contains("never stores secrets"),
        "{error}"
    );
    assert_eq!(store.get(added.id).unwrap(), Some(added.clone()));
    assert_eq!(
        change_log::changes_of_note(&conn, added.id).unwrap().len(),
        1
    );
}

#[test]
fn delete_writes_change_log_and_can_be_restored() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();

    let deleted = store.delete(added.id, Actor::User, &at(TUESDAY)).unwrap();

    assert_eq!(deleted, added);
    assert_eq!(store.get(added.id).unwrap(), None);
    let log = change_log::changes_of_note(&conn, added.id).unwrap();
    let deletion = log.last().unwrap();
    assert_eq!(deletion.action, Action::Delete);
    assert_eq!(deletion.note_before().unwrap(), Some(added.clone()));

    let restored = store
        .restore(deletion.id, Actor::User, &at(TUESDAY))
        .unwrap();

    assert_eq!(restored, added);
    assert_eq!(store.get(added.id).unwrap(), Some(added.clone()));
    let log = change_log::changes_of_note(&conn, added.id).unwrap();
    assert_eq!(log.last().unwrap().action, Action::Restore);
}

#[test]
fn delete_of_missing_note_fails() {
    let conn = db::open_in_memory().unwrap();

    let error = NoteStore::new(&conn)
        .delete(99, Actor::User, &at(MONDAY))
        .unwrap_err();

    assert_eq!(error.to_string(), "note #99 not found");
}

#[test]
fn only_a_deletion_can_be_restored_and_only_once() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();
    let addition = change_log::changes_of_note(&conn, added.id).unwrap()[0].id;
    assert!(store.restore(addition, Actor::User, &at(TUESDAY)).is_err());

    store.delete(added.id, Actor::User, &at(TUESDAY)).unwrap();
    let deletion = change_log::changes_of_note(&conn, added.id).unwrap()[1].id;
    store.restore(deletion, Actor::User, &at(TUESDAY)).unwrap();

    assert!(store.restore(deletion, Actor::User, &at(TUESDAY)).is_err());
    assert!(store.restore(404, Actor::User, &at(TUESDAY)).is_err());
}

#[test]
fn change_log_records_actor() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();
    store.delete(added.id, Actor::User, &at(TUESDAY)).unwrap();

    let actors: Vec<Actor> = change_log::changes_of_note(&conn, added.id)
        .unwrap()
        .iter()
        .map(|change| change.actor)
        .collect();

    assert_eq!(actors, [Actor::Claude, Actor::User]);
}

#[test]
fn id_of_a_deleted_note_is_never_reused() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let first = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();
    store.delete(first.id, Actor::User, &at(MONDAY)).unwrap();

    let second = store
        .add(&retry_draft(), Actor::Claude, &at(TUESDAY))
        .unwrap();

    assert_ne!(second.id, first.id);
}

#[test]
fn expires_on_is_saved_and_can_be_edited() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let mut fields = retry_fields();
    fields.expires_on = Some("2026-10-12".parse().unwrap());

    let added = store
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap();
    assert_eq!(added.expires_on, Some("2026-10-12".parse().unwrap()));
    assert_eq!(store.get(added.id).unwrap(), Some(added.clone()));

    let changes = NoteChanges {
        expires_on: Some("2026-10-19".parse().unwrap()),
        ..NoteChanges::default()
    };
    let edited = store
        .edit(added.id, &changes, Actor::User, &at(TUESDAY))
        .unwrap();
    assert_eq!(edited.expires_on, Some("2026-10-19".parse().unwrap()));
    assert_eq!(store.get(added.id).unwrap(), Some(edited));
}

fn commitment_fields() -> NoteFields {
    NoteFields {
        note_type: Some(NoteType::Commitment),
        ..retry_fields()
    }
}

fn add_person(conn: &rusqlite::Connection, id: i64, name: &str) {
    // People commands come in step 5.
    conn.execute(
        "INSERT INTO people (id, name) VALUES (?1, ?2)",
        rusqlite::params![id, name],
    )
    .unwrap();
}

#[test]
fn owner_and_planned_date_are_saved_and_read_back() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    add_person(&conn, 7, "Anna Nowak");
    add_person(&conn, 9, "Piotr Zielinski");
    let mut fields = commitment_fields();
    fields.owner = Some(7);
    fields.planned_for = Some("2026-10-08".parse().unwrap());

    let added = store
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap();

    assert_eq!(added.owner, Some(7));
    assert_eq!(added.planned_for, Some("2026-10-08".parse().unwrap()));
    assert_eq!(store.get(added.id).unwrap(), Some(added.clone()));

    let changes = NoteChanges {
        owner: Some(9),
        ..NoteChanges::default()
    };
    let edited = store
        .edit(added.id, &changes, Actor::User, &at(TUESDAY))
        .unwrap();
    assert_eq!(edited.owner, Some(9));
    assert_eq!(store.get(added.id).unwrap(), Some(edited));
}

#[test]
fn unknown_owner_is_refused_with_a_clear_message() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    add_person(&conn, 7, "Anna Nowak");
    let mut fields = commitment_fields();
    fields.owner = Some(8);

    let error = store
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap_err();
    assert_eq!(error.to_string(), "person #8 not found");
    assert_eq!(store.get(1).unwrap(), None);

    let mine = store
        .add(
            &NoteDraft::new(commitment_fields()).unwrap(),
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap();
    let changes = NoteChanges {
        owner: Some(8),
        ..NoteChanges::default()
    };
    let error = store
        .edit(mine.id, &changes, Actor::User, &at(TUESDAY))
        .unwrap_err();
    assert_eq!(error.to_string(), "person #8 not found");
}

#[test]
fn change_applies_the_rule_saves_and_logs_the_previous_state() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(
            &NoteDraft::new(commitment_fields()).unwrap(),
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap();

    let changed = store
        .change(added.id, Action::Done, Actor::User, &at(TUESDAY), |note| {
            Ok(Note {
                commitment_status: Some(CommitmentStatus::Done),
                closed_at: Some(TUESDAY.into()),
                planned_for: Some("2026-10-06".parse().unwrap()),
                ..note.clone()
            })
        })
        .unwrap();

    assert_eq!(changed.commitment_status, Some(CommitmentStatus::Done));
    assert_eq!(changed.closed_at.as_deref(), Some(TUESDAY));
    assert_eq!(changed.updated_at, TUESDAY);
    assert_eq!(store.get(added.id).unwrap(), Some(changed));
    let log = change_log::changes_of_note(&conn, added.id).unwrap();
    assert_eq!(log.len(), 2);
    assert_eq!(log[1].action, Action::Done);
    assert_eq!(log[1].actor, Actor::User);
    assert_eq!(log[1].note_before().unwrap(), Some(added));
}

#[test]
fn refused_change_saves_nothing() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let added = store
        .add(&retry_draft(), Actor::Claude, &at(MONDAY))
        .unwrap();

    let error = store
        .change(added.id, Action::Done, Actor::User, &at(TUESDAY), |_| {
            anyhow::bail!("the rule says no")
        })
        .unwrap_err();

    assert_eq!(error.to_string(), "the rule says no");
    assert_eq!(store.get(added.id).unwrap(), Some(added.clone()));
    assert_eq!(
        change_log::changes_of_note(&conn, added.id).unwrap().len(),
        1
    );
    assert_eq!(
        store
            .change(99, Action::Done, Actor::User, &at(TUESDAY), |note| Ok(
                note.clone()
            ))
            .unwrap_err()
            .to_string(),
        "note #99 not found"
    );
}

fn fields_with_people(people: &[i64]) -> NoteFields {
    NoteFields {
        people: people.to_vec(),
        ..retry_fields()
    }
}

#[test]
fn note_people_are_saved_and_read_back() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    add_person(&conn, 7, "Anna Nowak");
    add_person(&conn, 9, "Piotr Zielinski");

    let added = store
        .add(
            &NoteDraft::new(fields_with_people(&[9, 7])).unwrap(),
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap();

    assert_eq!(added.people, [9, 7]);
    assert_eq!(store.get(added.id).unwrap(), Some(added));
}

#[test]
fn unknown_person_on_a_note_is_refused() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    add_person(&conn, 7, "Anna Nowak");

    let error = store
        .add(
            &NoteDraft::new(fields_with_people(&[7, 8])).unwrap(),
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap_err();

    assert_eq!(error.to_string(), "person #8 not found");
    assert_eq!(store.get(1).unwrap(), None);
}

#[test]
fn edit_replaces_the_people_of_a_note() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    add_person(&conn, 7, "Anna Nowak");
    add_person(&conn, 9, "Piotr Zielinski");
    let added = store
        .add(
            &NoteDraft::new(fields_with_people(&[7])).unwrap(),
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap();

    let changes = NoteChanges {
        people: Some(vec![9]),
        ..NoteChanges::default()
    };
    let edited = store
        .edit(added.id, &changes, Actor::User, &at(TUESDAY))
        .unwrap();

    assert_eq!(edited.people, [9]);
    assert_eq!(store.get(added.id).unwrap().unwrap().people, [9]);
    let unknown = NoteChanges {
        people: Some(vec![8]),
        ..NoteChanges::default()
    };
    assert!(
        store
            .edit(added.id, &unknown, Actor::User, &at(TUESDAY))
            .is_err()
    );
    // An edit of another field keeps the people.
    let title = NoteChanges {
        title: Some("New title".into()),
        ..NoteChanges::default()
    };
    assert_eq!(
        store
            .edit(added.id, &title, Actor::User, &at(TUESDAY))
            .unwrap()
            .people,
        [9]
    );
}

#[test]
fn deleted_note_is_restored_with_its_people() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    add_person(&conn, 7, "Anna Nowak");
    let added = store
        .add(
            &NoteDraft::new(fields_with_people(&[7])).unwrap(),
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap();
    store.delete(added.id, Actor::User, &at(TUESDAY)).unwrap();
    let deletion = change_log::changes_of_note(&conn, added.id)
        .unwrap()
        .pop()
        .unwrap();

    let restored = store
        .restore(deletion.id, Actor::User, &at(TUESDAY))
        .unwrap();

    assert_eq!(restored.people, [7]);
    assert_eq!(store.get(added.id).unwrap(), Some(added));
}

// ----- replace and links -----

fn draft(title: &str) -> NoteDraft {
    NoteDraft::new(NoteFields {
        title: title.into(),
        ..retry_fields()
    })
    .unwrap()
}

fn note_count(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
        .unwrap()
}

fn actions(conn: &rusqlite::Connection, note_id: i64) -> Vec<Action> {
    change_log::changes_of_note(conn, note_id)
        .unwrap()
        .iter()
        .map(|change| change.action)
        .collect()
}

#[test]
fn replace_marks_old_outdated_and_links_it() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let old = store
        .add(&draft("Retry 3 times"), Actor::Claude, &at(MONDAY))
        .unwrap();

    let new = store
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();

    assert_eq!(new.title, "Retry 5 times");
    assert_eq!(new.status, NoteStatus::Active);
    assert_eq!(new.replaces, [old.id]);
    assert_eq!(new.created_at, TUESDAY);
    assert_eq!(store.get(new.id).unwrap(), Some(new.clone()));
    let old_now = store.get(old.id).unwrap().unwrap();
    assert_eq!(
        old_now,
        Note {
            status: NoteStatus::Outdated,
            replaced_by: Some(new.id),
            updated_at: TUESDAY.into(),
            ..old.clone()
        }
    );
    // Both changes are in the log, the old note with its state before.
    assert_eq!(actions(&conn, new.id), [Action::Add]);
    assert_eq!(actions(&conn, old.id), [Action::Add, Action::Replace]);
    let replace = change_log::changes_of_note(&conn, old.id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(replace.note_before().unwrap(), Some(old));
}

#[test]
fn replace_copying_takes_fields_from_the_old_note() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let old = store
        .add(&draft("Retry 3 times"), Actor::Claude, &at(MONDAY))
        .unwrap();

    let new = store
        .replace_copying(
            old.id,
            Replacement::new("Retry 5 times", "Now 5 retries."),
            Actor::Claude,
            &at(TUESDAY),
        )
        .unwrap();

    assert_eq!(new.body, "Now 5 retries.");
    assert_eq!(new.repos, old.repos);
    assert_eq!(new.tickets, old.tickets);
    assert_eq!(new.source, old.source);
    assert_eq!(new.replaces, [old.id]);
    let missing = store.replace_copying(
        99,
        Replacement::new("Retry 7 times", "Body."),
        Actor::Claude,
        &at(TUESDAY),
    );
    assert_eq!(missing.unwrap_err().to_string(), "note #99 not found");
}

#[test]
fn replace_of_outdated_note_is_refused() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let old = store
        .add(&draft("Retry 3 times"), Actor::Claude, &at(MONDAY))
        .unwrap();
    let new = store
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();

    let error = store
        .replace(old.id, &draft("Retry 7 times"), Actor::Claude, &at(TUESDAY))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        format!(
            "note #{} is already outdated, replaced by #{}",
            old.id, new.id
        )
    );
}

#[test]
fn replace_saves_new_note_and_old_change_in_one_transaction() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let old = store
        .add(&draft("Retry 3 times"), Actor::Claude, &at(MONDAY))
        .unwrap();
    store
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();
    let notes_before = note_count(&conn);

    // The old note cannot be replaced again, so the new note must not be saved either.
    assert!(
        store
            .replace(old.id, &draft("Retry 7 times"), Actor::Claude, &at(TUESDAY))
            .is_err()
    );
    assert!(
        store
            .replace(99, &draft("Retry 7 times"), Actor::Claude, &at(TUESDAY))
            .is_err()
    );

    assert_eq!(note_count(&conn), notes_before);
    assert_eq!(actions(&conn, old.id), [Action::Add, Action::Replace]);
    // And the connection is usable again: no transaction was left open.
    store
        .add(&draft("Another note"), Actor::Claude, &at(TUESDAY))
        .unwrap();
}

#[test]
fn related_link_shows_on_both_notes() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let first = store
        .add(&draft("Retry 5 times"), Actor::Claude, &at(MONDAY))
        .unwrap();
    let second = store
        .add(&draft("Retry budget"), Actor::Claude, &at(MONDAY))
        .unwrap();
    let third = store
        .add(&draft("Backoff"), Actor::Claude, &at(MONDAY))
        .unwrap();

    assert!(
        store
            .link(second.id, first.id, Actor::Claude, &at(TUESDAY))
            .unwrap()
    );
    assert!(
        store
            .link(second.id, third.id, Actor::Claude, &at(TUESDAY))
            .unwrap()
    );

    assert_eq!(store.get(first.id).unwrap().unwrap().related, [second.id]);
    assert_eq!(
        store.get(second.id).unwrap().unwrap().related,
        [first.id, third.id]
    );
    assert_eq!(store.get(third.id).unwrap().unwrap().related, [second.id]);

    // Linking twice, in any direction, changes nothing and is not logged.
    assert!(
        !store
            .link(first.id, second.id, Actor::Claude, &at(TUESDAY))
            .unwrap()
    );
    assert_eq!(
        actions(&conn, second.id),
        [Action::Add, Action::Link, Action::Link]
    );
    // The link is a change of both notes.
    assert_eq!(actions(&conn, first.id), [Action::Add, Action::Link]);

    let itself = store
        .link(first.id, first.id, Actor::Claude, &at(TUESDAY))
        .unwrap_err();
    assert_eq!(itself.to_string(), "a note can never be linked to itself");
    let missing = store
        .link(first.id, 99, Actor::Claude, &at(TUESDAY))
        .unwrap_err();
    assert_eq!(missing.to_string(), "note #99 not found");
}

#[test]
fn note_that_replaced_another_cannot_be_deleted() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let old = store
        .add(&draft("Retry 3 times"), Actor::Claude, &at(MONDAY))
        .unwrap();
    let new = store
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();

    let error = store.delete(new.id, Actor::User, &at(TUESDAY)).unwrap_err();

    assert_eq!(
        error.to_string(),
        format!(
            "note #{} replaced #{}: undo the replace, or replace #{}",
            new.id, old.id, new.id
        )
    );
    assert!(store.get(new.id).unwrap().is_some());
    // The outdated note itself can be deleted: it was replaced, nothing points to it as true.
    store.delete(old.id, Actor::User, &at(TUESDAY)).unwrap();
    assert_eq!(
        store.get(new.id).unwrap().unwrap().replaces,
        Vec::<i64>::new()
    );
}

#[test]
fn deleted_note_is_restored_with_its_links() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let old = store
        .add(&draft("Retry 3 times"), Actor::Claude, &at(MONDAY))
        .unwrap();
    let new = store
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();
    let other = store
        .add(&draft("Retry budget"), Actor::Claude, &at(MONDAY))
        .unwrap();
    store
        .link(old.id, other.id, Actor::Claude, &at(TUESDAY))
        .unwrap();
    let old_before = store.get(old.id).unwrap().unwrap();
    store.delete(old.id, Actor::User, &at(TUESDAY)).unwrap();
    let deletion = change_log::changes_of_note(&conn, old.id)
        .unwrap()
        .pop()
        .unwrap();

    let restored = store
        .restore(deletion.id, Actor::User, &at(TUESDAY))
        .unwrap();

    assert_eq!(restored, old_before);
    assert_eq!(store.get(old.id).unwrap(), Some(old_before));
    assert_eq!(store.get(new.id).unwrap().unwrap().replaces, [old.id]);
    assert_eq!(store.get(other.id).unwrap().unwrap().related, [old.id]);
}

#[test]
fn links_to_a_deleted_note_disappear() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let first = store
        .add(&draft("Retry 5 times"), Actor::Claude, &at(MONDAY))
        .unwrap();
    let second = store
        .add(&draft("Retry budget"), Actor::Claude, &at(MONDAY))
        .unwrap();
    store
        .link(first.id, second.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    store.delete(second.id, Actor::User, &at(TUESDAY)).unwrap();

    assert!(store.get(first.id).unwrap().unwrap().related.is_empty());
    // Restoring a note whose linked note is gone drops that link.
    let linked = store.get(first.id).unwrap().unwrap();
    store.delete(first.id, Actor::User, &at(TUESDAY)).unwrap();
    let deletion = change_log::changes_of_note(&conn, first.id)
        .unwrap()
        .pop()
        .unwrap();
    let restored = store
        .restore(deletion.id, Actor::User, &at(TUESDAY))
        .unwrap();
    assert_eq!(restored, linked);
}

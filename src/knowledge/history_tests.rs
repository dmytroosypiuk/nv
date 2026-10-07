use super::*;
use crate::commitments::{mark_done, postpone};
use crate::db;
use crate::knowledge::change_log::{self, Action, Actor};
use crate::knowledge::note::{
    CommitmentStatus, Note, NoteChanges, NoteDraft, NoteStatus, NoteType,
};
use crate::knowledge::people::PeopleStore;
use crate::knowledge::store::NoteStore;
use crate::test_support::{MONDAY, at, work_note};

const TUESDAY: &str = "2026-10-06T10:30:00+02:00";
const WEDNESDAY: &str = "2026-10-07T08:00:00+02:00";

fn draft(title: &str) -> NoteDraft {
    NoteDraft::new(work_note(title, "Body.")).unwrap()
}

fn add(conn: &Connection, title: &str) -> Note {
    NoteStore::new(conn)
        .add(&draft(title), Actor::Claude, &at(MONDAY))
        .unwrap()
}

fn add_commitment(conn: &Connection, title: &str, planned: &str) -> Note {
    let mut fields = work_note(title, "Promised.");
    fields.note_type = Some(NoteType::Commitment);
    fields.planned_for = Some(planned.parse().unwrap());
    NoteStore::new(conn)
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap()
}

fn retitle(conn: &Connection, id: i64, title: &str) -> Note {
    let changes = NoteChanges {
        title: Some(title.into()),
        ..NoteChanges::default()
    };
    NoteStore::new(conn)
        .edit(id, &changes, Actor::User, &at(TUESDAY))
        .unwrap()
}

fn get(conn: &Connection, id: i64) -> Option<Note> {
    NoteStore::new(conn).get(id).unwrap()
}

fn undo_last(conn: &Connection) -> UndoOutcome {
    undo(conn, None, Actor::User, &at(WEDNESDAY)).unwrap()
}

fn undo_error(conn: &Connection, change_id: Option<i64>) -> String {
    undo(conn, change_id, Actor::User, &at(WEDNESDAY))
        .unwrap_err()
        .to_string()
}

fn last_change_id(conn: &Connection) -> i64 {
    conn.query_row("SELECT max(id) FROM change_log", [], |row| row.get(0))
        .unwrap()
}

fn all(conn: &Connection) -> Vec<HistoryEntry> {
    history(conn, HistoryFilter::default(), 100).unwrap()
}

// ----- history -----

#[test]
fn history_lists_changes_newest_first_with_titles() {
    let conn = db::open_in_memory().unwrap();
    let note = add(&conn, "Retry 3 times");
    retitle(&conn, note.id, "Retry 5 times");
    let anna = PeopleStore::new(&conn)
        .add("Anna Nowak", None, &[], Actor::Claude, &at(TUESDAY))
        .unwrap();

    let entries = all(&conn);

    assert_eq!(
        entries,
        [
            HistoryEntry {
                id: 3,
                at: TUESDAY.into(),
                actor: Actor::Claude,
                action: Action::PersonAdd,
                subject: "person",
                subject_id: anna.id,
                title: Some("Anna Nowak".into()),
                undone: false,
            },
            HistoryEntry {
                id: 2,
                at: TUESDAY.into(),
                actor: Actor::User,
                action: Action::Edit,
                subject: "note",
                subject_id: note.id,
                title: Some("Retry 5 times".into()),
                undone: false,
            },
            HistoryEntry {
                id: 1,
                at: MONDAY.into(),
                actor: Actor::Claude,
                action: Action::Add,
                subject: "note",
                subject_id: note.id,
                title: Some("Retry 5 times".into()),
                undone: false,
            },
        ]
    );
    assert_eq!(
        history(&conn, HistoryFilter::default(), 2).unwrap().len(),
        2
    );
}

#[test]
fn history_of_one_note_or_one_person() {
    let conn = db::open_in_memory().unwrap();
    let first = add(&conn, "Retry 3 times");
    let second = add(&conn, "Staging postgres");
    retitle(&conn, first.id, "Retry 5 times");
    let anna = PeopleStore::new(&conn)
        .add("Anna Nowak", None, &[], Actor::Claude, &at(TUESDAY))
        .unwrap();

    let of_first = history(
        &conn,
        HistoryFilter {
            note: Some(first.id),
            person: None,
        },
        100,
    )
    .unwrap();
    let of_anna = history(
        &conn,
        HistoryFilter {
            note: None,
            person: Some(anna.id),
        },
        100,
    )
    .unwrap();

    let ids: Vec<i64> = of_first.iter().map(|entry| entry.id).collect();
    assert_eq!(ids, [3, 1]);
    assert_eq!(of_anna.len(), 1);
    assert_eq!(of_anna[0].action, Action::PersonAdd);
    assert_eq!(
        history(
            &conn,
            HistoryFilter {
                note: Some(second.id),
                person: None
            },
            100
        )
        .unwrap()
        .len(),
        1
    );
}

#[test]
fn history_shows_title_of_a_deleted_note() {
    let conn = db::open_in_memory().unwrap();
    let note = add(&conn, "Retry 3 times");
    NoteStore::new(&conn)
        .delete(note.id, Actor::User, &at(TUESDAY))
        .unwrap();

    let entries = all(&conn);

    assert_eq!(entries[0].action, Action::Delete);
    assert_eq!(entries[0].title.as_deref(), Some("Retry 3 times"));
    // The add has no state before, but the note is known from the deletion.
    assert_eq!(entries[1].title.as_deref(), Some("Retry 3 times"));
}

// ----- undo of note changes -----

#[test]
fn undo_restores_previous_state_of_last_change() {
    let conn = db::open_in_memory().unwrap();
    let before = add(&conn, "Retry 3 times");
    retitle(&conn, before.id, "Retry 5 times");

    let outcome = undo_last(&conn);

    assert_eq!(outcome.description, format!("edit of note #{}", before.id));
    assert!(outcome.text_changed);
    assert!(outcome.warnings.is_empty());
    assert_eq!(
        get(&conn, before.id),
        Some(Note {
            updated_at: WEDNESDAY.into(),
            ..before
        })
    );
}

#[test]
fn undo_restores_deleted_note() {
    let conn = db::open_in_memory().unwrap();
    let mut fields = work_note("Retry 3 times", "Body.");
    fields.repos = vec!["billing-api".into()];
    fields.tickets = vec!["PAY-1234".into()];
    let note = NoteStore::new(&conn)
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap();
    NoteStore::new(&conn)
        .delete(note.id, Actor::User, &at(TUESDAY))
        .unwrap();

    let outcome = undo_last(&conn);

    assert_eq!(outcome.description, format!("delete of note #{}", note.id));
    assert!(outcome.text_changed);
    assert_eq!(get(&conn, note.id), Some(note));
}

#[test]
fn undo_of_add_removes_the_note() {
    let conn = db::open_in_memory().unwrap();
    let note = add(&conn, "Saved by mistake");

    let outcome = undo_last(&conn);

    assert_eq!(outcome.description, format!("add of note #{}", note.id));
    assert_eq!(get(&conn, note.id), None);
    // Nothing is lost: the undo entry keeps the note.
    let undo_entry = change_log::changes_of_note(&conn, note.id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(undo_entry.action, Action::Undo);
    assert_eq!(undo_entry.note_before().unwrap(), Some(note));
}

#[test]
fn undo_of_done_reopens_the_commitment() {
    let conn = db::open_in_memory().unwrap();
    let commitment = add_commitment(&conn, "Send retry numbers", "2026-10-07");
    let now = at(TUESDAY);
    NoteStore::new(&conn)
        .change(commitment.id, Action::Done, Actor::User, &now, |note| {
            Ok(mark_done(note, &now)?)
        })
        .unwrap();

    let outcome = undo_last(&conn);

    let reopened = get(&conn, commitment.id).unwrap();
    assert_eq!(reopened.commitment_status, Some(CommitmentStatus::Todo));
    assert_eq!(reopened.closed_at, None);
    // The text did not change: nothing to embed.
    assert!(!outcome.text_changed);
}

#[test]
fn undo_of_postpone_restores_the_planned_date() {
    let conn = db::open_in_memory().unwrap();
    let commitment = add_commitment(&conn, "Send retry numbers", "2026-10-07");
    NoteStore::new(&conn)
        .change(
            commitment.id,
            Action::Postpone,
            Actor::User,
            &at(TUESDAY),
            |note| Ok(postpone(note, "2026-10-09".parse().unwrap())?),
        )
        .unwrap();

    undo_last(&conn);

    assert_eq!(
        get(&conn, commitment.id).unwrap().planned_for,
        Some("2026-10-07".parse().unwrap())
    );
}

#[test]
fn undo_of_replace_removes_new_note_and_reactivates_old() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let old = add(&conn, "Retry 3 times");
    let new = store
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();

    let outcome = undo_last(&conn);

    assert_eq!(outcome.description, format!("replace of note #{}", old.id));
    assert_eq!(get(&conn, new.id), None);
    let old_now = get(&conn, old.id).unwrap();
    assert_eq!(old_now.status, NoteStatus::Active);
    assert_eq!(old_now.replaced_by, None);
    // Both entries of the replace are marked: the add of the new note too.
    // Then two undo entries: one keeps the removed new note, one the old note's state.
    let undone: Vec<bool> = all(&conn).iter().rev().map(|entry| entry.undone).collect();
    assert_eq!(undone, [false, true, true, false, false]);
    let kept = change_log::changes_of_note(&conn, new.id)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(kept.note_before().unwrap().unwrap().title, "Retry 5 times");
    // And the old note can be replaced again.
    store
        .replace(
            old.id,
            &draft("Retry 7 times"),
            Actor::Claude,
            &at(WEDNESDAY),
        )
        .unwrap();
}

#[test]
fn undo_of_replace_is_refused_when_new_note_changed() {
    let conn = db::open_in_memory().unwrap();
    let old = add(&conn, "Retry 3 times");
    let new = NoteStore::new(&conn)
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();
    let replace = last_change_id(&conn);
    retitle(&conn, new.id, "Retry 5 times for billing-api");
    let edit = last_change_id(&conn);

    assert_eq!(
        undo_error(&conn, Some(replace)),
        format!(
            "note #{} changed after change #{replace}: undo change #{edit} first",
            new.id
        )
    );
    assert!(get(&conn, new.id).is_some());

    // After the edit is undone, the replace can be undone.
    undo(&conn, Some(edit), Actor::User, &at(WEDNESDAY)).unwrap();
    undo(&conn, Some(replace), Actor::User, &at(WEDNESDAY)).unwrap();
    assert_eq!(get(&conn, new.id), None);
}

#[test]
fn undo_of_add_is_refused_for_a_note_that_replaced_another() {
    let conn = db::open_in_memory().unwrap();
    let old = add(&conn, "Retry 3 times");
    let new = NoteStore::new(&conn)
        .replace(old.id, &draft("Retry 5 times"), Actor::Claude, &at(TUESDAY))
        .unwrap();
    let replace = last_change_id(&conn);
    let add_of_new = replace - 1;

    assert_eq!(
        undo_error(&conn, Some(add_of_new)),
        format!(
            "note #{} replaced #{}: undo change #{replace} instead",
            new.id, old.id
        )
    );
}

#[test]
fn undo_of_link_removes_the_link_on_both_notes() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let first = add(&conn, "Retry 5 times");
    let second = add(&conn, "Retry budget");
    let third = add(&conn, "Backoff");
    store
        .link(first.id, third.id, Actor::Claude, &at(TUESDAY))
        .unwrap();
    store
        .link(first.id, second.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    let outcome = undo_last(&conn);

    assert_eq!(
        outcome.description,
        format!("link of note #{} and note #{}", first.id, second.id)
    );
    assert!(!outcome.text_changed);
    assert_eq!(get(&conn, first.id).unwrap().related, [third.id]);
    assert!(get(&conn, second.id).unwrap().related.is_empty());
    assert_eq!(get(&conn, third.id).unwrap().related, [first.id]);
    // The next undo takes the first link, not the other half of the second one.
    undo_last(&conn);
    assert!(get(&conn, first.id).unwrap().related.is_empty());
    assert!(get(&conn, third.id).unwrap().related.is_empty());
}

// ----- undo of people changes -----

#[test]
fn undo_of_merge_brings_the_person_back() {
    let conn = db::open_in_memory().unwrap();
    let people = PeopleStore::new(&conn);
    let nowak = people
        .add("Anna Nowak", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap();
    let duplicate = people
        .add("Anna N.", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap();
    let mut fields = work_note("Anna prefers async reviews", "Body.");
    fields.people = vec![duplicate.id];
    let note = NoteStore::new(&conn)
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap();
    people
        .merge(nowak.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    let outcome = undo_last(&conn);

    assert_eq!(
        outcome.description,
        format!("merge of person #{}", nowak.id)
    );
    assert_eq!(people.get(duplicate.id).unwrap(), Some(duplicate.clone()));
    assert_eq!(people.get(nowak.id).unwrap(), Some(nowak));
    assert_eq!(get(&conn, note.id).unwrap().people, [duplicate.id]);
    // One undo entry, and the merge is marked.
    let entries = all(&conn);
    assert_eq!(entries[0].action, Action::Undo);
    assert_eq!(entries[1].action, Action::Merge);
    assert!(entries[1].undone);
}

#[test]
fn undo_of_alias_and_person_edit_restores_the_person() {
    let conn = db::open_in_memory().unwrap();
    let people = PeopleStore::new(&conn);
    let anna = people
        .add(
            "Anna Nowak",
            Some("tester"),
            &[],
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap();
    let with_alias = people
        .add_alias(anna.id, "Anna", Actor::Claude, &at(TUESDAY))
        .unwrap();
    people
        .edit(
            anna.id,
            Some("Anna Maria Nowak"),
            Some("QA lead"),
            Actor::User,
            &at(TUESDAY),
        )
        .unwrap();

    let outcome = undo_last(&conn);
    assert_eq!(
        outcome.description,
        format!("person-edit of person #{}", anna.id)
    );
    assert_eq!(people.get(anna.id).unwrap(), Some(with_alias));

    undo_last(&conn);
    assert_eq!(people.get(anna.id).unwrap(), Some(anna.clone()));

    // Nobody uses the person: the add can be undone too.
    undo_last(&conn);
    assert_eq!(people.get(anna.id).unwrap(), None);
}

#[test]
fn undo_of_person_add_is_refused_when_notes_use_the_person() {
    let conn = db::open_in_memory().unwrap();
    let anna = PeopleStore::new(&conn)
        .add("Anna Nowak", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap();
    let person_add = last_change_id(&conn);
    let mut fields = work_note("Anna prefers async reviews", "Body.");
    fields.people = vec![anna.id];
    NoteStore::new(&conn)
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap();

    assert_eq!(
        undo_error(&conn, Some(person_add)),
        format!("person #{} is used by notes: it cannot be removed", anna.id)
    );
}

// ----- which change can be undone -----

#[test]
fn only_the_last_change_of_a_note_can_be_undone() {
    let conn = db::open_in_memory().unwrap();
    let note = add(&conn, "Retry 3 times");
    retitle(&conn, note.id, "Retry 4 times");
    let first_edit = last_change_id(&conn);
    retitle(&conn, note.id, "Retry 5 times");
    let second_edit = last_change_id(&conn);

    assert_eq!(
        undo_error(&conn, Some(first_edit)),
        format!(
            "note #{} changed after change #{first_edit}: undo change #{second_edit} first",
            note.id
        )
    );
    assert_eq!(get(&conn, note.id).unwrap().title, "Retry 5 times");

    undo(&conn, Some(second_edit), Actor::User, &at(WEDNESDAY)).unwrap();
    undo(&conn, Some(first_edit), Actor::User, &at(WEDNESDAY)).unwrap();
    assert_eq!(get(&conn, note.id).unwrap().title, "Retry 3 times");
}

#[test]
fn undo_without_id_takes_the_newest_change_not_yet_undone() {
    let conn = db::open_in_memory().unwrap();
    let first = add(&conn, "Retry 3 times");
    let second = add(&conn, "Staging postgres");
    retitle(&conn, first.id, "Retry 5 times");
    retitle(&conn, second.id, "Staging database");

    let outcomes: Vec<String> = (0..3).map(|_| undo_last(&conn).description).collect();

    assert_eq!(
        outcomes,
        [
            format!("edit of note #{}", second.id),
            format!("edit of note #{}", first.id),
            format!("add of note #{}", second.id),
        ]
    );
    assert_eq!(get(&conn, first.id).unwrap().title, "Retry 3 times");
    assert_eq!(get(&conn, second.id), None);
}

#[test]
fn undone_change_is_marked_and_cannot_be_undone_again() {
    let conn = db::open_in_memory().unwrap();
    let note = add(&conn, "Retry 3 times");
    retitle(&conn, note.id, "Retry 5 times");
    let edit = last_change_id(&conn);

    let outcome = undo(&conn, Some(edit), Actor::User, &at(WEDNESDAY)).unwrap();

    assert_eq!(outcome.change.id, edit);
    let entries = all(&conn);
    assert_eq!(entries[0].action, Action::Undo);
    assert_eq!(entries[0].actor, Actor::User);
    assert_eq!(entries[0].at, WEDNESDAY);
    assert!(!entries[0].undone);
    assert!(entries[1].undone);
    assert_eq!(
        change_log::change(&conn, edit)
            .unwrap()
            .unwrap()
            .undone_at
            .as_deref(),
        Some(WEDNESDAY)
    );
    assert_eq!(
        undo_error(&conn, Some(edit)),
        format!("change #{edit} is already undone")
    );
}

#[test]
fn an_undo_cannot_be_undone() {
    let conn = db::open_in_memory().unwrap();
    let note = add(&conn, "Retry 3 times");
    retitle(&conn, note.id, "Retry 5 times");
    undo_last(&conn);
    let the_undo = last_change_id(&conn);

    assert_eq!(
        undo_error(&conn, Some(the_undo)),
        format!("change #{the_undo} is an undo: it cannot be undone")
    );
    assert_eq!(undo_error(&conn, Some(404)), "change #404 not found");
}

#[test]
fn undo_with_nothing_to_undo_says_so() {
    let conn = db::open_in_memory().unwrap();
    assert_eq!(undo_error(&conn, None), "nothing to undo");

    add(&conn, "Retry 3 times");
    undo_last(&conn);

    assert_eq!(undo_error(&conn, None), "nothing to undo");
}

#[test]
fn undo_drops_people_that_no_longer_exist() {
    let conn = db::open_in_memory().unwrap();
    let people = PeopleStore::new(&conn);
    let nowak = people
        .add("Anna Nowak", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap();
    let duplicate = people
        .add("Anna N.", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap();
    let mut fields = work_note("Anna prefers async reviews", "Body.");
    fields.people = vec![duplicate.id, nowak.id];
    let note = NoteStore::new(&conn)
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap();
    NoteStore::new(&conn)
        .delete(note.id, Actor::User, &at(TUESDAY))
        .unwrap();
    let deletion = last_change_id(&conn);
    people
        .merge(nowak.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    let outcome = undo(&conn, Some(deletion), Actor::User, &at(WEDNESDAY)).unwrap();

    assert_eq!(get(&conn, note.id).unwrap().people, [nowak.id]);
    assert_eq!(
        outcome.warnings,
        [format!(
            "person #{} no longer exists: left out of note #{}",
            duplicate.id, note.id
        )]
    );
}

#[test]
fn undo_saves_everything_or_nothing() {
    let conn = db::open_in_memory().unwrap();
    let people = PeopleStore::new(&conn);
    let anna = people
        .add("Anna Nowak", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap();
    let duplicate = people
        .add("Anna N.", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap();
    let mut fields = work_note("Review the retry PR", "Promised.");
    fields.note_type = Some(NoteType::Commitment);
    fields.owner = Some(duplicate.id);
    let commitment = NoteStore::new(&conn)
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap();
    NoteStore::new(&conn)
        .delete(commitment.id, Actor::User, &at(TUESDAY))
        .unwrap();
    let deletion = last_change_id(&conn);
    people
        .merge(anna.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();
    let entries_before = all(&conn);

    // The owner is gone: the note cannot come back as it was.
    assert_eq!(
        undo_error(&conn, Some(deletion)),
        format!("person #{} not found", duplicate.id)
    );

    assert_eq!(get(&conn, commitment.id), None);
    assert_eq!(all(&conn), entries_before);
}

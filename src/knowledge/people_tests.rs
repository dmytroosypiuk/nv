use super::*;
use crate::db;
use crate::knowledge::change_log::{self, Action};
use crate::knowledge::person::PersonError;
use crate::test_support::{MONDAY, at};

fn aliases(aliases: &[&str]) -> Vec<String> {
    aliases.iter().map(|alias| alias.to_string()).collect()
}

fn add(store: &PeopleStore<'_>, name: &str, role: Option<&str>, with: &[&str]) -> Person {
    store
        .add(name, role, &aliases(with), Actor::Claude, &at(MONDAY))
        .unwrap()
}

fn person_error(error: anyhow::Error) -> PersonError {
    error.downcast::<PersonError>().unwrap()
}

#[test]
fn added_person_can_be_read_back_with_aliases_and_role() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);

    let anna = add(
        &store,
        " Anna  Nowak ",
        Some("QA lead"),
        &["Anna", "anna.nowak@contoso.com"],
    );

    assert_eq!(
        anna,
        Person {
            id: 1,
            name: "Anna Nowak".into(),
            role: Some("QA lead".into()),
            aliases: aliases(&["Anna", "anna.nowak@contoso.com"]),
        }
    );
    assert_eq!(store.get(1).unwrap(), Some(anna));
    assert_eq!(store.get(2).unwrap(), None);
    assert_eq!(
        person_name(&conn, 1).unwrap().as_deref(),
        Some("Anna Nowak")
    );
}

#[test]
fn person_requires_a_name() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);

    let error = store
        .add("  ", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap_err();

    assert_eq!(person_error(error), PersonError::MissingName);
    assert!(store.list().unwrap().is_empty());
}

#[test]
fn main_name_belongs_to_one_person() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    add(&store, "Anna Nowak", None, &[]);

    let error = store
        .add("anna NOWAK", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "'anna NOWAK' is already person #1 Anna Nowak"
    );
    // Also for a one-word main name.
    add(&store, "Madonna", None, &[]);
    assert!(
        store
            .add("madonna", None, &[], Actor::Claude, &at(MONDAY))
            .is_err()
    );
}

#[test]
fn full_alias_belongs_to_one_person() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    add(
        &store,
        "Anna Nowak",
        None,
        &["Anna N.", "anna.nowak@contoso.com"],
    );
    let other = add(&store, "Anna Kowalska", None, &[]);

    let error = store
        .add_alias(
            other.id,
            "ANNA.NOWAK@contoso.com",
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "alias 'ANNA.NOWAK@contoso.com' already belongs to person #1 Anna Nowak"
    );
    // The same rule when a new person arrives with that alias.
    let error = store
        .add(
            "Ann Novak",
            None,
            &aliases(&["Anna N."]),
            Actor::Claude,
            &at(MONDAY),
        )
        .unwrap_err();
    assert_eq!(
        person_error(error),
        PersonError::AliasTaken {
            alias: "Anna N.".into(),
            id: 1,
            owner: "Anna Nowak".into()
        }
    );
    assert_eq!(store.list().unwrap().len(), 2);
}

#[test]
fn full_alias_cannot_be_another_persons_main_name() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    add(&store, "Anna Nowak", None, &["Anna Maria Nowak"]);
    let other = add(&store, "Anna Kowalska", None, &[]);

    assert!(
        store
            .add_alias(other.id, "anna nowak", Actor::Claude, &at(MONDAY))
            .is_err()
    );
    // And a new main name cannot be someone's full alias.
    let error = store
        .add("Anna Maria Nowak", None, &[], Actor::Claude, &at(MONDAY))
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "'Anna Maria Nowak' is already person #1 Anna Nowak"
    );
}

#[test]
fn short_alias_can_match_two_people() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(&store, "Anna Nowak", None, &["Anna"]);
    let kowalska = add(&store, "Anna Kowalska", None, &[]);

    let kowalska = store
        .add_alias(kowalska.id, "Anna", Actor::Claude, &at(MONDAY))
        .unwrap();

    assert_eq!(kowalska.aliases, ["Anna"]);
    assert_eq!(store.get(nowak.id).unwrap().unwrap().aliases, ["Anna"]);
}

#[test]
fn same_alias_twice_on_one_person_is_kept_once() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let anna = add(
        &store,
        "Anna Nowak",
        None,
        &["Anna", "anna", "Anna Nowak", " "],
    );
    assert_eq!(anna.aliases, ["Anna"]);

    let anna = store
        .add_alias(anna.id, " ANNA ", Actor::Claude, &at(MONDAY))
        .unwrap();
    assert_eq!(anna.aliases, ["Anna"]);

    let error = store
        .add_alias(anna.id, "  ", Actor::Claude, &at(MONDAY))
        .unwrap_err();
    assert_eq!(person_error(error), PersonError::BlankAlias);
    let error = store
        .add_alias(99, "Ania", Actor::Claude, &at(MONDAY))
        .unwrap_err();
    assert_eq!(error.to_string(), "person #99 not found");
}

#[test]
fn people_search_shows_all_matches_with_aliases_and_role() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(
        &store,
        "Anna Nowak",
        Some("QA lead"),
        &["Anna", "anna.nowak@contoso.com"],
    );
    let kowalska = add(
        &store,
        "Anna Kowalska",
        Some("backend developer"),
        &["Anna", "Ania"],
    );
    add(&store, "Piotr Zielinski", None, &[]);

    let found = store.search("Anna").unwrap();

    // By main name.
    assert_eq!(found, [kowalska, nowak]);
    assert_eq!(found[1].role.as_deref(), Some("QA lead"));
    assert_eq!(found[1].aliases, ["Anna", "anna.nowak@contoso.com"]);
}

#[test]
fn people_search_ignores_case_and_matches_part_of_a_name() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(&store, "Anna Nowak", None, &["anna.nowak@contoso.com"]);
    let kowalska = add(&store, "Anna Kowalska", None, &["Ania"]);

    assert_eq!(store.search("NOWAK").unwrap(), std::slice::from_ref(&nowak));
    assert_eq!(store.search("contoso").unwrap(), [nowak]);
    assert_eq!(store.search("ani").unwrap(), [kowalska]);
    assert!(store.search("Piotr").unwrap().is_empty());
    assert_eq!(store.search("").unwrap().len(), 2);
}

#[test]
fn list_gives_everyone_by_main_name() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    add(&store, "Piotr Zielinski", None, &[]);
    add(&store, "anna Nowak", None, &[]);
    add(&store, "Anna Kowalska", None, &[]);

    let names: Vec<String> = store
        .list()
        .unwrap()
        .into_iter()
        .map(|person| person.name)
        .collect();

    assert_eq!(names, ["Anna Kowalska", "anna Nowak", "Piotr Zielinski"]);
}

#[test]
fn edit_renames_person_and_keeps_old_name_as_alias() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let anna = add(&store, "Anna Novak", Some("tester"), &["Anna"]);
    add(&store, "Anna Kowalska", None, &[]);

    let renamed = store
        .edit(anna.id, Some("Anna Nowak"), None, Actor::User, &at(MONDAY))
        .unwrap();
    assert_eq!(
        renamed,
        Person {
            id: anna.id,
            name: "Anna Nowak".into(),
            role: Some("tester".into()),
            aliases: aliases(&["Anna", "Anna Novak"]),
        }
    );

    let promoted = store
        .edit(anna.id, None, Some("QA lead"), Actor::User, &at(MONDAY))
        .unwrap();
    assert_eq!(promoted.role.as_deref(), Some("QA lead"));
    assert_eq!(promoted.name, "Anna Nowak");
    assert_eq!(store.get(anna.id).unwrap(), Some(promoted));

    // An alias that becomes the main name is no longer an alias.
    let back = store
        .edit(anna.id, Some("Anna Novak"), None, Actor::User, &at(MONDAY))
        .unwrap();
    assert_eq!(back.aliases, ["Anna", "Anna Nowak"]);

    let taken = store.edit(
        anna.id,
        Some("anna kowalska"),
        None,
        Actor::User,
        &at(MONDAY),
    );
    assert!(taken.is_err());
    let nothing = store
        .edit(anna.id, None, None, Actor::User, &at(MONDAY))
        .unwrap_err();
    assert_eq!(person_error(nothing), PersonError::NothingToChange);
}

#[test]
fn people_changes_are_in_the_change_log() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let anna = add(&store, "Anna Nowak", None, &[]);
    let with_alias = store
        .add_alias(anna.id, "Anna", Actor::User, &at(MONDAY))
        .unwrap();
    store
        .edit(anna.id, None, Some("QA lead"), Actor::User, &at(MONDAY))
        .unwrap();
    // An alias the person already has changes nothing and is not logged.
    store
        .add_alias(anna.id, "anna", Actor::User, &at(MONDAY))
        .unwrap();

    let log = change_log::changes_of_person(&conn, anna.id).unwrap();

    let actions: Vec<Action> = log.iter().map(|change| change.action).collect();
    assert_eq!(
        actions,
        [Action::PersonAdd, Action::Alias, Action::PersonEdit]
    );
    assert_eq!(log[0].before_json, None);
    assert_eq!(log[0].actor, Actor::Claude);
    assert_eq!(log[0].note_id, None);
    let before_alias: Person = serde_json::from_str(log[1].before_json.as_ref().unwrap()).unwrap();
    assert_eq!(before_alias, anna);
    let before_edit: Person = serde_json::from_str(log[2].before_json.as_ref().unwrap()).unwrap();
    assert_eq!(before_edit, with_alias);
}

// ----- merge -----

use crate::knowledge::note::{NoteDraft, NoteType};
use crate::knowledge::store::NoteStore;
use crate::test_support::work_note;

const TUESDAY: &str = "2026-10-06T10:30:00+02:00";

/// Saves a note about the given people; with `owner`, a commitment that person owns.
fn note_about(conn: &Connection, title: &str, people: &[i64], owner: Option<i64>) -> i64 {
    let mut fields = work_note(title, "Body.");
    fields.people = people.to_vec();
    if owner.is_some() {
        fields.note_type = Some(NoteType::Commitment);
        fields.owner = owner;
    }
    NoteStore::new(conn)
        .add(&NoteDraft::new(fields).unwrap(), Actor::Claude, &at(MONDAY))
        .unwrap()
        .id
}

fn people_of(conn: &Connection, note_id: i64) -> Vec<i64> {
    NoteStore::new(conn).get(note_id).unwrap().unwrap().people
}

fn owner_of(conn: &Connection, note_id: i64) -> Option<i64> {
    NoteStore::new(conn).get(note_id).unwrap().unwrap().owner
}

fn last_merge(conn: &Connection, kept: i64) -> i64 {
    change_log::changes_of_person(conn, kept)
        .unwrap()
        .into_iter()
        .rfind(|change| change.action == Action::Merge)
        .unwrap()
        .id
}

#[test]
fn merge_moves_aliases_and_notes_and_is_undoable() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(&store, "Anna Nowak", Some("QA lead"), &["Anna"]);
    let duplicate = add(&store, "Anna N.", None, &["Anna", "anna.nowak@contoso.com"]);
    let hers = note_about(&conn, "Anna prefers async reviews", &[duplicate.id], None);
    let untouched = note_about(&conn, "Anna owns the QA board", &[nowak.id], None);

    let merged = store
        .merge(nowak.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    assert_eq!(
        merged,
        Person {
            id: nowak.id,
            name: "Anna Nowak".into(),
            role: Some("QA lead".into()),
            aliases: aliases(&["Anna", "Anna N.", "anna.nowak@contoso.com"]),
        }
    );
    assert_eq!(store.get(nowak.id).unwrap(), Some(merged));
    assert_eq!(store.get(duplicate.id).unwrap(), None);
    assert_eq!(people_of(&conn, hers), [nowak.id]);
    assert_eq!(people_of(&conn, untouched), [nowak.id]);

    let (kept, returned) = store
        .undo_merge(last_merge(&conn, nowak.id), Actor::User, &at(TUESDAY))
        .unwrap();

    assert_eq!(kept, nowak);
    assert_eq!(returned, duplicate);
    assert_eq!(store.get(nowak.id).unwrap(), Some(nowak.clone()));
    assert_eq!(store.get(duplicate.id).unwrap(), Some(duplicate.clone()));
    assert_eq!(people_of(&conn, hers), [duplicate.id]);
    assert_eq!(people_of(&conn, untouched), [nowak.id]);
}

#[test]
fn merge_moves_owned_commitments() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(&store, "Anna Nowak", None, &[]);
    let duplicate = add(&store, "Anna N.", None, &[]);
    let owed = note_about(&conn, "Review the retry PR", &[], Some(duplicate.id));
    let already_hers = note_about(&conn, "Send the test plan", &[], Some(nowak.id));

    store
        .merge(nowak.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();
    assert_eq!(owner_of(&conn, owed), Some(nowak.id));

    store
        .undo_merge(last_merge(&conn, nowak.id), Actor::User, &at(TUESDAY))
        .unwrap();
    assert_eq!(owner_of(&conn, owed), Some(duplicate.id));
    assert_eq!(owner_of(&conn, already_hers), Some(nowak.id));
}

#[test]
fn merge_of_people_linked_to_the_same_note_keeps_one_link() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(&store, "Anna Nowak", None, &[]);
    let piotr = add(&store, "Piotr Zielinski", None, &[]);
    let duplicate = add(&store, "Anna N.", None, &[]);
    let shared = note_about(
        &conn,
        "Retry meeting",
        &[duplicate.id, piotr.id, nowak.id],
        None,
    );

    store
        .merge(nowak.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    let mut people = people_of(&conn, shared);
    people.sort();
    assert_eq!(people, [nowak.id, piotr.id]);

    store
        .undo_merge(last_merge(&conn, nowak.id), Actor::User, &at(TUESDAY))
        .unwrap();
    let mut people = people_of(&conn, shared);
    people.sort();
    assert_eq!(people, [nowak.id, piotr.id, duplicate.id]);
}

#[test]
fn merge_keeps_role_of_kept_person_or_takes_the_other() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let with_role = add(&store, "Anna Nowak", Some("QA lead"), &[]);
    let other_role = add(&store, "Anna N.", Some("tester"), &[]);
    let no_role = add(&store, "Piotr Zielinski", None, &[]);
    let has_role = add(&store, "Piotr Z.", Some("DevOps"), &[]);

    let anna = store
        .merge(with_role.id, other_role.id, Actor::Claude, &at(TUESDAY))
        .unwrap();
    let piotr = store
        .merge(no_role.id, has_role.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    assert_eq!(anna.role.as_deref(), Some("QA lead"));
    assert_eq!(piotr.role.as_deref(), Some("DevOps"));
}

#[test]
fn person_cannot_be_merged_into_itself() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let anna = add(&store, "Anna Nowak", None, &[]);

    let error = store
        .merge(anna.id, anna.id, Actor::Claude, &at(TUESDAY))
        .unwrap_err();

    assert_eq!(person_error(error), PersonError::SamePerson);
    assert_eq!(store.get(anna.id).unwrap(), Some(anna));
}

#[test]
fn merge_with_unknown_person_fails_and_changes_nothing() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let anna = add(&store, "Anna Nowak", None, &["Anna"]);

    let error = store
        .merge(anna.id, 99, Actor::Claude, &at(TUESDAY))
        .unwrap_err();
    assert_eq!(error.to_string(), "person #99 not found");
    let error = store
        .merge(99, anna.id, Actor::Claude, &at(TUESDAY))
        .unwrap_err();
    assert_eq!(error.to_string(), "person #99 not found");

    assert_eq!(store.list().unwrap(), std::slice::from_ref(&anna));
    assert_eq!(
        change_log::changes_of_person(&conn, anna.id).unwrap().len(),
        1
    );
}

#[test]
fn id_of_a_merged_person_is_never_reused() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(&store, "Anna Nowak", None, &[]);
    let duplicate = add(&store, "Anna N.", None, &[]);
    store
        .merge(nowak.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();

    let piotr = add(&store, "Piotr Zielinski", None, &[]);

    assert_ne!(piotr.id, duplicate.id);
}

#[test]
fn merge_can_be_undone_only_once_and_only_a_merge() {
    let conn = db::open_in_memory().unwrap();
    let store = PeopleStore::new(&conn);
    let nowak = add(&store, "Anna Nowak", None, &[]);
    let duplicate = add(&store, "Anna N.", None, &[]);
    store
        .merge(nowak.id, duplicate.id, Actor::Claude, &at(TUESDAY))
        .unwrap();
    let merge = last_merge(&conn, nowak.id);
    let addition = change_log::changes_of_person(&conn, nowak.id).unwrap()[0].id;

    store.undo_merge(merge, Actor::User, &at(TUESDAY)).unwrap();

    let again = store
        .undo_merge(merge, Actor::User, &at(TUESDAY))
        .unwrap_err();
    assert_eq!(again.to_string(), "person #2 already exists");
    let not_a_merge = store
        .undo_merge(addition, Actor::User, &at(TUESDAY))
        .unwrap_err();
    assert_eq!(
        not_a_merge.to_string(),
        format!("change #{addition} is not a merge")
    );
    assert!(store.undo_merge(404, Actor::User, &at(TUESDAY)).is_err());
    // The undo itself is in the change log.
    let log = change_log::changes_of_person(&conn, nowak.id).unwrap();
    assert_eq!(log.last().unwrap().action, Action::Undo);
}

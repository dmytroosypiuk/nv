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

    assert_eq!(store.search("NOWAK").unwrap(), [nowak.clone()]);
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

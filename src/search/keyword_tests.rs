use super::*;
use crate::clock::Now;
use crate::db;
use crate::knowledge::change_log::Actor;
use crate::knowledge::note::{Area, NoteChanges, NoteDraft, NoteFields};
use crate::knowledge::store::NoteStore;

fn now() -> Now {
    Now::parse("2026-10-07T09:00:00+02:00").unwrap()
}

fn add(conn: &Connection, title: &str, body: &str) -> i64 {
    let draft = NoteDraft::new(NoteFields {
        title: title.into(),
        body: body.into(),
        area: Area::Work,
        note_type: None,
        project: None,
        repos: vec![],
        tickets: vec![],
        source: None,
        expires_on: None,
    })
    .unwrap();
    NoteStore::new(conn)
        .add(&draft, Actor::Claude, &now())
        .unwrap()
        .id
}

#[test]
fn search_finds_note_by_keyword() {
    let conn = db::open_in_memory().unwrap();
    let retry = add(&conn, "Retry 5 times", "For billing-api calls.");
    add(
        &conn,
        "Staging postgres",
        "Connect through the bastion host.",
    );

    assert_eq!(keyword_search(&conn, "retry", 5).unwrap(), [retry]);
}

#[test]
fn search_finds_word_in_title_or_body() {
    let conn = db::open_in_memory().unwrap();
    let in_title = add(&conn, "Bastion host address", "It changed last week.");
    let in_body = add(
        &conn,
        "Staging postgres",
        "Connect through the bastion host.",
    );
    add(&conn, "Retry 5 times", "For billing-api calls.");

    let mut found = keyword_search(&conn, "bastion", 5).unwrap();
    found.sort();

    assert_eq!(found, [in_title, in_body]);
}

#[test]
fn title_match_ranks_above_body_match() {
    let conn = db::open_in_memory().unwrap();
    let in_body = add(
        &conn,
        "Staging postgres",
        "Connect through the bastion host.",
    );
    let in_title = add(&conn, "Bastion host address", "It changed last week.");

    assert_eq!(
        keyword_search(&conn, "bastion", 5).unwrap(),
        [in_title, in_body]
    );
}

#[test]
fn any_word_of_the_query_is_enough() {
    let conn = db::open_in_memory().unwrap();
    let retry = add(&conn, "Retry 5 times", "For billing-api calls.");

    let found = keyword_search(&conn, "how many times do we retry billing calls", 5).unwrap();

    assert_eq!(found, [retry]);
}

#[test]
fn search_matches_other_word_forms() {
    let conn = db::open_in_memory().unwrap();
    let retry = add(&conn, "Retry 5 times", "For billing-api calls.");

    assert_eq!(keyword_search(&conn, "retries", 5).unwrap(), [retry]);
    assert_eq!(keyword_search(&conn, "RETRYING", 5).unwrap(), [retry]);
}

#[test]
fn search_survives_quotes_and_fts_operators_in_query() {
    let conn = db::open_in_memory().unwrap();
    let retry = add(&conn, "Retry 5 times", "For billing-api calls.");

    for query in [
        "\"retry",
        "retry AND",
        "retry OR NOT (",
        "title:retry*",
        "billing-api",
        "retry^2 NEAR(x",
    ] {
        assert_eq!(keyword_search(&conn, query, 5).unwrap(), [retry], "{query}");
    }
}

#[test]
fn query_without_words_finds_nothing() {
    let conn = db::open_in_memory().unwrap();
    add(&conn, "Retry 5 times", "For billing-api calls.");

    assert!(keyword_search(&conn, "", 5).unwrap().is_empty());
    assert!(keyword_search(&conn, " \"*()- ", 5).unwrap().is_empty());
}

#[test]
fn edited_note_is_found_by_new_text_not_old() {
    let conn = db::open_in_memory().unwrap();
    let id = add(&conn, "Retry 5 times", "For billing-api calls.");
    let changes = NoteChanges {
        title: Some("Backoff is exponential".into()),
        ..NoteChanges::default()
    };
    NoteStore::new(&conn)
        .edit(id, &changes, Actor::User, &now())
        .unwrap();

    assert_eq!(keyword_search(&conn, "exponential", 5).unwrap(), [id]);
    assert!(keyword_search(&conn, "retry", 5).unwrap().is_empty());
}

#[test]
fn deleted_note_is_not_found_and_restored_note_is_found_again() {
    let conn = db::open_in_memory().unwrap();
    let store = NoteStore::new(&conn);
    let id = add(&conn, "Retry 5 times", "For billing-api calls.");

    store.delete(id, Actor::User, &now()).unwrap();
    assert!(keyword_search(&conn, "retry", 5).unwrap().is_empty());

    let deletion: i64 = conn
        .query_row("SELECT max(id) FROM change_log", [], |row| row.get(0))
        .unwrap();
    store.restore(deletion, Actor::User, &now()).unwrap();
    assert_eq!(keyword_search(&conn, "retry", 5).unwrap(), [id]);
}

#[test]
fn search_respects_limit() {
    let conn = db::open_in_memory().unwrap();
    for number in 1..=4 {
        add(&conn, &format!("Retry note {number}"), "Body.");
    }

    assert_eq!(keyword_search(&conn, "retry", 3).unwrap().len(), 3);
}

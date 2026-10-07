use super::*;
use crate::db;
use crate::knowledge::change_log::Actor;
use crate::knowledge::note::NoteChanges;
use crate::knowledge::store::NoteStore;
use crate::search::embedder::fake::{FAKE_MODEL, FakeEmbedder};
use crate::test_support::{MONDAY, add_note, at};

fn edit(conn: &Connection, id: i64, changes: NoteChanges) {
    NoteStore::new(conn)
        .edit(id, &changes, Actor::User, &at(MONDAY))
        .unwrap();
}

fn pending_ids(conn: &Connection) -> Vec<i64> {
    pending_notes(conn, FAKE_MODEL)
        .unwrap()
        .iter()
        .map(|pending| pending.note_id)
        .collect()
}

#[test]
fn vector_survives_the_round_trip_to_a_blob() {
    let vector = vec![0.25f32, -1.0, 0.0, 3.5e-7];

    let blob = to_blob(&vector);

    assert_eq!(blob.len(), 16);
    assert_eq!(from_blob(&blob).unwrap(), vector);
    assert!(from_blob(&blob[..15]).is_err());
}

#[test]
fn embedded_text_is_title_then_body_without_prefix() {
    assert_eq!(
        embedding_text("Retry 5 times", "For billing."),
        "Retry 5 times\nFor billing."
    );
}

#[test]
fn text_hash_changes_with_the_text() {
    assert_eq!(text_hash("a"), text_hash("a"));
    assert_ne!(text_hash("a"), text_hash("b"));
    assert_eq!(text_hash("a").len(), 64);
}

#[test]
fn new_note_is_pending() {
    let conn = db::open_in_memory().unwrap();
    let id = add_note(&conn, "Retry 5 times", "For billing-api calls.");

    let pending = pending_notes(&conn, FAKE_MODEL).unwrap();

    assert_eq!(
        pending,
        [PendingNote {
            note_id: id,
            text: "Retry 5 times\nFor billing-api calls.".into(),
            text_hash: text_hash("Retry 5 times\nFor billing-api calls."),
        }]
    );
}

#[test]
fn embedded_note_is_not_pending() {
    let conn = db::open_in_memory().unwrap();
    add_note(&conn, "Retry 5 times", "For billing-api calls.");

    let embedded = embed_pending(&conn, &mut FakeEmbedder::default()).unwrap();

    assert_eq!(embedded, 1);
    assert!(pending_ids(&conn).is_empty());
    assert_eq!(embedded_count(&conn, FAKE_MODEL).unwrap(), 1);
}

#[test]
fn edited_note_text_changes_hash_and_becomes_pending() {
    let conn = db::open_in_memory().unwrap();
    let id = add_note(&conn, "Retry 5 times", "For billing-api calls.");
    embed_pending(&conn, &mut FakeEmbedder::default()).unwrap();

    edit(
        &conn,
        id,
        NoteChanges {
            body: Some("Now 7 retries.".into()),
            ..NoteChanges::default()
        },
    );

    assert_eq!(pending_ids(&conn), [id]);
}

#[test]
fn edit_of_other_fields_keeps_the_embedding() {
    let conn = db::open_in_memory().unwrap();
    let id = add_note(&conn, "Retry 5 times", "For billing-api calls.");
    embed_pending(&conn, &mut FakeEmbedder::default()).unwrap();

    edit(
        &conn,
        id,
        NoteChanges {
            project: Some("Billing".into()),
            ..NoteChanges::default()
        },
    );

    assert!(pending_ids(&conn).is_empty());
}

#[test]
fn deleted_note_loses_its_embedding() {
    let conn = db::open_in_memory().unwrap();
    let id = add_note(&conn, "Retry 5 times", "For billing-api calls.");
    embed_pending(&conn, &mut FakeEmbedder::default()).unwrap();

    NoteStore::new(&conn)
        .delete(id, Actor::User, &at(MONDAY))
        .unwrap();

    assert_eq!(embedded_count(&conn, FAKE_MODEL).unwrap(), 0);
}

#[test]
fn embed_pending_embeds_only_pending_notes() {
    let conn = db::open_in_memory().unwrap();
    let mut embedder = FakeEmbedder::default();
    add_note(&conn, "Retry 5 times", "For billing-api calls.");
    embed_pending(&conn, &mut embedder).unwrap();
    add_note(
        &conn,
        "Staging postgres",
        "Connect through the bastion host.",
    );

    let embedded = embed_pending(&conn, &mut embedder).unwrap();

    assert_eq!(embedded, 1);
    assert_eq!(
        embedder.embedded,
        [
            "Retry 5 times\nFor billing-api calls.",
            "Staging postgres\nConnect through the bastion host."
        ]
    );
    assert_eq!(embed_pending(&conn, &mut embedder).unwrap(), 0);
    assert_eq!(embedder.embedded.len(), 2);
}

#[test]
fn embed_pending_handles_more_notes_than_one_batch() {
    let conn = db::open_in_memory().unwrap();
    for number in 0..70 {
        add_note(&conn, &format!("Note {number}"), "Body.");
    }

    assert_eq!(
        embed_pending(&conn, &mut FakeEmbedder::default()).unwrap(),
        70
    );
    assert_eq!(embedded_count(&conn, FAKE_MODEL).unwrap(), 70);
}

#[test]
fn one_embedding_per_note_per_model() {
    let conn = db::open_in_memory().unwrap();
    let id = add_note(&conn, "Retry 5 times", "For billing-api calls.");
    let mut embedder = FakeEmbedder::default();
    embed_pending(&conn, &mut embedder).unwrap();
    edit(
        &conn,
        id,
        NoteChanges {
            body: Some("Now 7 retries.".into()),
            ..NoteChanges::default()
        },
    );

    embed_pending(&conn, &mut embedder).unwrap();

    assert_eq!(embedded_count(&conn, FAKE_MODEL).unwrap(), 1);
    let (dims, bytes): (i64, i64) = conn
        .query_row("SELECT dims, length(vector) FROM embeddings", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(dims as usize, embedder.dims());
    assert_eq!(bytes as usize, embedder.dims() * 4);
    // A note embedded with another model is still pending for this one.
    assert_eq!(pending_notes(&conn, "another-model").unwrap().len(), 1);
}

#[test]
fn cleared_model_makes_every_note_pending_again() {
    let conn = db::open_in_memory().unwrap();
    let id = add_note(&conn, "Retry 5 times", "For billing-api calls.");
    embed_pending(&conn, &mut FakeEmbedder::default()).unwrap();

    clear(&conn, FAKE_MODEL).unwrap();

    assert_eq!(pending_ids(&conn), [id]);
}

#[test]
fn vector_search_ranks_by_cosine() {
    let conn = db::open_in_memory().unwrap();
    let mut embedder = FakeEmbedder::default();
    let postgres = add_note(
        &conn,
        "Staging postgres",
        "Connect through the bastion host.",
    );
    let retry = add_note(&conn, "Retry 5 times", "For billing calls.");
    let gift = add_note(&conn, "Gift for grandfather", "A chess set.");
    embed_pending(&conn, &mut embedder).unwrap();
    let query = embedder
        .embed(&["retry billing calls".to_string()])
        .unwrap()
        .remove(0);

    let found = vector_search(&conn, FAKE_MODEL, &query, None, 10).unwrap();

    let ids: Vec<i64> = found.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids[0], retry);
    assert_eq!(ids.len(), 3);
    assert!(found[0].1 > found[1].1);
    assert!(ids.contains(&postgres) && ids.contains(&gift));

    let only_gift = HashSet::from([gift]);
    let found = vector_search(&conn, FAKE_MODEL, &query, Some(&only_gift), 10).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].0, gift);

    assert_eq!(
        vector_search(&conn, FAKE_MODEL, &query, None, 1)
            .unwrap()
            .len(),
        1
    );
}

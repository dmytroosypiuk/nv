use super::embedder::fake::FakeLoader;
use super::*;
use crate::db;
use crate::knowledge::note::NoteType;
use crate::test_support::{MONDAY, add_at, add_note, work_note};

const TUESDAY: &str = "2026-10-06T10:30:00+02:00";

fn request(text: Option<&str>) -> SearchRequest<'_> {
    SearchRequest {
        text,
        filter: NoteFilter::default(),
        limit: 5,
        today: "2026-10-07".parse().unwrap(),
    }
}

fn found(conn: &Connection, request: &SearchRequest<'_>, loader: &mut FakeLoader) -> Vec<i64> {
    search(conn, request, loader).unwrap().note_ids
}

fn mark_outdated(conn: &Connection, id: i64) {
    // Only the status matters here; a real replace would add one more note.
    conn.execute("UPDATE notes SET status = 'outdated' WHERE id = ?1", [id])
        .unwrap();
}

#[test]
fn search_embeds_pending_notes_before_ranking() {
    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    let retry = add_note(&conn, "Retry 5 times", "For billing calls.");

    let ids = found(&conn, &request(Some("retry billing")), &mut loader);

    assert_eq!(ids, [retry]);
    assert_eq!(loader.loads, 1);
    // First the pending note, then the query. The query gets no prefix.
    assert_eq!(
        loader.embedder.embedded,
        ["Retry 5 times\nFor billing calls.", "retry billing"]
    );
}

#[test]
fn search_finds_note_that_only_the_vector_ranker_knows() {
    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    let retry = add_note(&conn, "Retry 5 times", "For billing calls.");
    // Keyword search cannot see it: break the keyword index for this note only.
    conn.execute(
        "INSERT INTO notes_fts (notes_fts) VALUES ('delete-all')",
        [],
    )
    .unwrap();

    assert_eq!(found(&conn, &request(Some("retry")), &mut loader), [retry]);
}

#[test]
fn search_finds_note_that_only_the_keyword_ranker_knows() {
    let conn = db::open_in_memory().unwrap();
    let retry = add_note(&conn, "Retry 5 times", "For billing calls.");
    let mut loader = FakeLoader::missing();

    let outcome = search(&conn, &request(Some("retries")), &mut loader).unwrap();

    assert_eq!(outcome.note_ids, [retry]);
}

#[test]
fn search_without_model_uses_keywords_only() {
    let conn = db::open_in_memory().unwrap();
    add_note(&conn, "Retry 5 times", "For billing calls.");
    add_note(&conn, "Staging postgres", "Bastion host.");
    let mut loader = FakeLoader::missing();

    let outcome = search(&conn, &request(Some("postgres")), &mut loader).unwrap();

    assert_eq!(outcome.note_ids.len(), 1);
    assert!(outcome.model_missing);
    assert!(loader.embedder.embedded.is_empty());

    let with_model = search(
        &conn,
        &request(Some("postgres")),
        &mut FakeLoader::default(),
    )
    .unwrap();
    assert!(!with_model.model_missing);
}

#[test]
fn note_found_by_keyword_and_vector_ranks_first() {
    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    add_note(
        &conn,
        "Staging postgres",
        "Connect through the bastion host.",
    );
    let locked = add_note(
        &conn,
        "Database is locked",
        "SQLite busy timeout was missing.",
    );
    add_note(&conn, "Gift for grandfather", "A chess set.");

    let ids = found(
        &conn,
        &request(Some("sqlite database locked error")),
        &mut loader,
    );

    assert_eq!(ids[0], locked);
    // The vector ranker has no cut-off: the other notes follow, Claude judges.
    assert_eq!(ids.len(), 3);
}

#[test]
fn active_note_ranks_above_outdated_with_same_score() {
    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    // Same text, so both rankers score them the same; the older one would come first or
    // second by chance.
    let old = add_at(
        &conn,
        work_note("Retry policy", "Retry billing calls."),
        MONDAY,
    );
    let new = add_at(
        &conn,
        work_note("Retry policy", "Retry billing calls."),
        TUESDAY,
    );
    let newest = add_at(
        &conn,
        work_note("Retry policy", "Retry billing calls."),
        TUESDAY,
    );
    mark_outdated(&conn, newest);
    mark_outdated(&conn, old);

    let ids = found(&conn, &request(Some("retry policy")), &mut loader);

    assert_eq!(ids[0], new);
    assert_eq!(ids.len(), 3);
}

#[test]
fn outdated_notes_still_appear() {
    let conn = db::open_in_memory().unwrap();
    let old = add_note(&conn, "Retry 3 times", "For billing calls.");
    mark_outdated(&conn, old);

    assert_eq!(
        found(&conn, &request(Some("retry")), &mut FakeLoader::default()),
        [old]
    );
    assert_eq!(
        found(&conn, &request(None), &mut FakeLoader::default()),
        [old]
    );
}

#[test]
fn filter_only_search_does_not_load_model() {
    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    let mut decision = work_note("Retry 5 times", "For billing calls.");
    decision.tickets = vec!["PAY-1234".into()];
    let decision = add_at(&conn, decision, MONDAY);
    add_note(&conn, "Staging postgres", "Bastion host.");
    let mut request = request(None);
    request.filter.ticket = Some("PAY-1234".into());

    let outcome = search(&conn, &request, &mut loader).unwrap();

    assert_eq!(outcome.note_ids, [decision]);
    assert!(!outcome.model_missing);
    assert_eq!(loader.loads, 0);

    // Blank text is no text.
    request.text = Some("  ");
    search(&conn, &request, &mut loader).unwrap();
    assert_eq!(loader.loads, 0);
}

#[test]
fn filter_only_search_lists_newest_first_and_active_above_outdated() {
    let conn = db::open_in_memory().unwrap();
    let monday = add_at(&conn, work_note("A", "a"), MONDAY);
    let tuesday = add_at(&conn, work_note("B", "b"), TUESDAY);
    let outdated = add_at(&conn, work_note("C", "c"), TUESDAY);
    mark_outdated(&conn, outdated);

    let ids = found(&conn, &request(None), &mut FakeLoader::default());

    assert_eq!(ids, [tuesday, monday, outdated]);
}

#[test]
fn filters_limit_text_search_too() {
    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    add_note(&conn, "Retry 5 times", "For billing calls.");
    let mut how_to = work_note("How to retry a failed job", "Press the retry button.");
    how_to.note_type = Some(NoteType::HowTo);
    let how_to = add_at(&conn, how_to, MONDAY);
    let mut request = request(Some("retry"));
    request.filter.note_type = Some(NoteType::HowTo);

    assert_eq!(found(&conn, &request, &mut loader), [how_to]);
}

#[test]
fn expired_note_is_not_found_by_text_unless_all() {
    let conn = db::open_in_memory().unwrap();
    let mut vacation = work_note("Anna is on vacation", "Back on Monday.");
    vacation.expires_on = Some("2026-10-06".parse().unwrap());
    let vacation = add_at(&conn, vacation, MONDAY);
    let mut request = request(Some("vacation"));

    assert!(found(&conn, &request, &mut FakeLoader::default()).is_empty());

    request.filter.include_expired = true;
    assert_eq!(
        found(&conn, &request, &mut FakeLoader::default()),
        [vacation]
    );
}

#[test]
fn search_respects_limit() {
    let conn = db::open_in_memory().unwrap();
    for number in 0..8 {
        add_note(&conn, &format!("Retry note {number}"), "Body.");
    }
    let mut request = request(Some("retry"));
    request.limit = 3;

    assert_eq!(found(&conn, &request, &mut FakeLoader::default()).len(), 3);
    request.text = None;
    assert_eq!(found(&conn, &request, &mut FakeLoader::default()).len(), 3);
}

#[test]
fn replaced_note_ranks_below_its_replacement() {
    use crate::clock::Now;
    use crate::knowledge::change_log::Actor;
    use crate::knowledge::note::NoteDraft;
    use crate::knowledge::store::NoteStore;

    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    // The old note matches the query better, word for word.
    let old = add_note(
        &conn,
        "Retry count for billing calls",
        "Retry billing calls 3 times.",
    );
    let new = NoteStore::new(&conn)
        .replace(
            old,
            &NoteDraft::new(work_note("Retry count", "Now 5 times.")).unwrap(),
            Actor::Claude,
            &Now::parse(TUESDAY).unwrap(),
        )
        .unwrap();

    let ids = found(&conn, &request(Some("retry billing calls")), &mut loader);

    assert_eq!(ids, [new.id, old]);
}

#[test]
fn filter_only_search_reports_total() {
    let conn = db::open_in_memory().unwrap();
    let mut loader = FakeLoader::default();
    for number in 1..=7 {
        add_note(&conn, &format!("Note {number}"), "Retry billing calls.");
    }

    let filter_only = search(&conn, &request(None), &mut loader).unwrap();
    // A text search ranks every note and has no cut-off: there is no total to report.
    let with_text = search(&conn, &request(Some("retry")), &mut loader).unwrap();

    assert_eq!(filter_only.note_ids.len(), 5);
    assert_eq!(filter_only.total, Some(7));
    assert_eq!(with_text.note_ids.len(), 5);
    assert_eq!(with_text.total, None);
}

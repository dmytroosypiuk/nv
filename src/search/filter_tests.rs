use super::*;
use crate::db;
use crate::knowledge::note::CommitmentStatus;
use crate::test_support::{MONDAY, add_at, add_note, work_note};

const TUESDAY: &str = "2026-10-06T10:30:00+02:00";
const WEDNESDAY: &str = "2026-10-07T08:00:00+02:00";

fn date(text: &str) -> Date {
    text.parse().unwrap()
}

fn ids(conn: &Connection, filter: &NoteFilter, today: &str) -> Vec<i64> {
    filter
        .candidates(conn, date(today))
        .unwrap()
        .iter()
        .map(|candidate| candidate.note_id)
        .collect()
}

#[test]
fn no_filter_gives_every_note_newest_first() {
    let conn = db::open_in_memory().unwrap();
    let monday = add_at(&conn, work_note("A", "a"), MONDAY);
    let wednesday = add_at(&conn, work_note("C", "c"), WEDNESDAY);
    let tuesday = add_at(&conn, work_note("B", "b"), TUESDAY);
    let same_time = add_at(&conn, work_note("D", "d"), TUESDAY);

    let filter = NoteFilter::default();

    assert!(!filter.narrows());
    assert_eq!(
        ids(&conn, &filter, "2026-10-07"),
        [wednesday, same_time, tuesday, monday]
    );
}

#[test]
fn filters_by_area_type_repo_and_ticket() {
    let conn = db::open_in_memory().unwrap();
    let mut learning = work_note("Backoff", "Wait longer each time.");
    learning.area = Area::Learning;
    let learning = add_at(&conn, learning, MONDAY);
    let mut decision = work_note("Retry 5 times", "For billing.");
    decision.note_type = Some(NoteType::Decision);
    decision.repos = vec!["billing-api".into(), "gateway".into()];
    decision.tickets = vec!["PAY-1234".into()];
    let decision = add_at(&conn, decision, MONDAY);
    add_note(&conn, "Staging postgres", "Bastion host.");

    let by = |filter: NoteFilter| {
        assert!(filter.narrows());
        ids(&conn, &filter, "2026-10-07")
    };

    assert_eq!(
        by(NoteFilter {
            area: Some(Area::Learning),
            ..NoteFilter::default()
        }),
        [learning]
    );
    assert_eq!(
        by(NoteFilter {
            note_type: Some(NoteType::Decision),
            ..NoteFilter::default()
        }),
        [decision]
    );
    assert_eq!(
        by(NoteFilter {
            repo: Some("gateway".into()),
            ..NoteFilter::default()
        }),
        [decision]
    );
    assert_eq!(
        by(NoteFilter {
            ticket: Some("PAY-1234".into()),
            ..NoteFilter::default()
        }),
        [decision]
    );
    assert!(
        by(NoteFilter {
            ticket: Some("PAY-9".into()),
            ..NoteFilter::default()
        })
        .is_empty()
    );
}

#[test]
fn filters_combine() {
    let conn = db::open_in_memory().unwrap();
    let mut both = work_note("Retry 5 times", "For billing.");
    both.note_type = Some(NoteType::Decision);
    both.repos = vec!["billing-api".into()];
    let both = add_at(&conn, both, MONDAY);
    let mut only_repo = work_note("Deploy steps", "Run the pipeline.");
    only_repo.repos = vec!["billing-api".into()];
    add_at(&conn, only_repo, MONDAY);

    let filter = NoteFilter {
        note_type: Some(NoteType::Decision),
        repo: Some("billing-api".into()),
        ..NoteFilter::default()
    };

    assert_eq!(ids(&conn, &filter, "2026-10-07"), [both]);
}

#[test]
fn since_keeps_notes_created_on_or_after_the_date() {
    let conn = db::open_in_memory().unwrap();
    add_at(&conn, work_note("A", "a"), MONDAY);
    let tuesday = add_at(&conn, work_note("B", "b"), TUESDAY);
    let wednesday = add_at(&conn, work_note("C", "c"), WEDNESDAY);

    let filter = NoteFilter {
        since: Some(date("2026-10-06")),
        ..NoteFilter::default()
    };

    assert_eq!(ids(&conn, &filter, "2026-10-07"), [wednesday, tuesday]);
}

#[test]
fn planned_keeps_commitments_planned_for_that_date() {
    let conn = db::open_in_memory().unwrap();
    let mut commitment = work_note("Send the report to Anna", "Promised on the daily.");
    commitment.note_type = Some(NoteType::Commitment);
    let tomorrow = add_at(&conn, commitment.clone(), MONDAY);
    let next_week = add_at(&conn, commitment, MONDAY);
    add_note(&conn, "Not a commitment", "Body.");
    // No command sets a planned date before step 4.
    conn.execute(
        "UPDATE notes SET planned_for = '2026-10-08' WHERE id = ?1",
        [tomorrow],
    )
    .unwrap();
    conn.execute(
        "UPDATE notes SET planned_for = '2026-10-15' WHERE id = ?1",
        [next_week],
    )
    .unwrap();

    let filter = NoteFilter {
        planned: Some(date("2026-10-08")),
        ..NoteFilter::default()
    };

    assert!(filter.narrows());
    assert_eq!(ids(&conn, &filter, "2026-10-07"), [tomorrow]);
}

#[test]
fn expired_notes_hidden_unless_all() {
    let conn = db::open_in_memory().unwrap();
    let mut vacation = work_note("Anna is on vacation", "Back on Monday.");
    vacation.expires_on = Some(date("2026-10-09"));
    let vacation = add_at(&conn, vacation, MONDAY);
    let lasting = add_note(&conn, "Retry 5 times", "For billing.");

    let hidden = NoteFilter::default();
    let all = NoteFilter {
        include_expired: true,
        ..NoteFilter::default()
    };

    assert_eq!(ids(&conn, &hidden, "2026-10-20"), [lasting]);
    assert_eq!(ids(&conn, &all, "2026-10-20"), [lasting, vacation]);
    assert!(!all.narrows());
}

#[test]
fn note_expires_the_day_after_expires_on() {
    let conn = db::open_in_memory().unwrap();
    let mut vacation = work_note("Anna is on vacation", "Back on Monday.");
    vacation.expires_on = Some(date("2026-10-09"));
    let vacation = add_at(&conn, vacation, MONDAY);
    let filter = NoteFilter::default();

    assert_eq!(ids(&conn, &filter, "2026-10-08"), [vacation]);
    assert_eq!(ids(&conn, &filter, "2026-10-09"), [vacation]);
    assert!(ids(&conn, &filter, "2026-10-10").is_empty());
}

#[test]
fn candidate_knows_if_the_note_is_outdated() {
    let conn = db::open_in_memory().unwrap();
    let old = add_note(&conn, "Retry 3 times", "For billing.");
    // Only the status matters here; a real replace would add one more note.
    conn.execute("UPDATE notes SET status = 'outdated' WHERE id = ?1", [old])
        .unwrap();

    let candidates = NoteFilter::default()
        .candidates(&conn, date("2026-10-07"))
        .unwrap();

    assert_eq!(
        candidates,
        [Candidate {
            note_id: old,
            outdated: true
        }]
    );
}

#[test]
fn person_filter_finds_linked_notes_and_owned_commitments() {
    let conn = db::open_in_memory().unwrap();
    conn.execute_batch(
        "INSERT INTO people (id, name) VALUES (7, 'Anna Nowak'), (9, 'Piotr Zielinski');",
    )
    .unwrap();
    let mut about_anna = work_note("Anna prefers async reviews", "No calls before 10.");
    about_anna.people = vec![7, 9];
    let about_anna = add_at(&conn, about_anna, MONDAY);
    let mut owed_by_anna = work_note("Review the retry PR", "Promised on the daily.");
    owed_by_anna.note_type = Some(NoteType::Commitment);
    owed_by_anna.owner = Some(7);
    let owed_by_anna = add_at(&conn, owed_by_anna, TUESDAY);
    let mut about_piotr = work_note("Piotr owns the gateway", "Ask him first.");
    about_piotr.people = vec![9];
    let about_piotr = add_at(&conn, about_piotr, WEDNESDAY);
    add_note(&conn, "Retry 5 times", "For billing.");

    let anna = NoteFilter {
        person: Some(7),
        ..NoteFilter::default()
    };
    let piotr = NoteFilter {
        person: Some(9),
        ..NoteFilter::default()
    };

    assert!(anna.narrows());
    assert_eq!(ids(&conn, &anna, "2026-10-07"), [owed_by_anna, about_anna]);
    assert_eq!(ids(&conn, &piotr, "2026-10-07"), [about_piotr, about_anna]);
}

#[test]
fn status_keeps_commitments_with_that_status() {
    let conn = db::open_in_memory().unwrap();
    let mut commitment = work_note("Send the report to Anna", "Promised on the daily.");
    commitment.note_type = Some(NoteType::Commitment);
    let open = add_at(&conn, commitment.clone(), MONDAY);
    let done = add_at(&conn, commitment.clone(), MONDAY);
    let dropped = add_at(&conn, commitment, MONDAY);
    for (status, id) in [("done", done), ("dropped", dropped)] {
        conn.execute(
            "UPDATE notes SET commitment_status = ?1, closed_at = ?2 WHERE id = ?3",
            params![status, TUESDAY, id],
        )
        .unwrap();
    }

    let by = |status| {
        let filter = NoteFilter {
            status: Some(status),
            ..NoteFilter::default()
        };
        assert!(filter.narrows());
        ids(&conn, &filter, "2026-10-07")
    };

    assert_eq!(by(CommitmentStatus::Todo), [open]);
    assert_eq!(by(CommitmentStatus::Done), [done]);
    assert_eq!(by(CommitmentStatus::Dropped), [dropped]);
}

#[test]
fn status_leaves_out_notes_that_are_not_commitments() {
    let conn = db::open_in_memory().unwrap();
    add_note(&conn, "Not a commitment", "Body.");

    let filter = NoteFilter {
        status: Some(CommitmentStatus::Todo),
        ..NoteFilter::default()
    };

    assert_eq!(ids(&conn, &filter, "2026-10-07"), [] as [i64; 0]);
}

#[test]
fn project_keeps_notes_of_that_project_ignoring_case() {
    let conn = db::open_in_memory().unwrap();
    let mut billing = work_note("Retry 5 times", "For billing.");
    billing.project = Some("Billing".into());
    let billing = add_at(&conn, billing, MONDAY);
    let mut other = work_note("Dark mode", "For the portal.");
    other.project = Some("Billing portal".into());
    add_at(&conn, other, MONDAY);
    add_note(&conn, "No project", "Body.");

    let filter = NoteFilter {
        project: Some("billing".into()),
        ..NoteFilter::default()
    };

    assert!(filter.narrows());
    assert_eq!(ids(&conn, &filter, "2026-10-07"), [billing]);
}

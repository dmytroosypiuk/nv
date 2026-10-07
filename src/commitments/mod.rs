//! Rules for commitment notes: status changes, postpone, today view.

pub mod today;

use thiserror::Error;

use crate::clock::{Date, Now};
use crate::knowledge::note::{CommitmentStatus, Note};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommitmentError {
    #[error("note #{id} is not a commitment")]
    NotACommitment { id: i64 },
    /// `done` and `dropped` are final: no postpone, no reopen.
    #[error("commitment #{id} is already {status}")]
    AlreadyClosed { id: i64, status: CommitmentStatus },
}

/// The promise was kept.
pub fn mark_done(commitment: &Note, now: &Now) -> Result<Note, CommitmentError> {
    close(commitment, CommitmentStatus::Done, now)
}

/// The promise will not be kept and nobody waits for it any more.
pub fn mark_dropped(commitment: &Note, now: &Now) -> Result<Note, CommitmentError> {
    close(commitment, CommitmentStatus::Dropped, now)
}

/// Moves the planned date; also gives a first date to a commitment that had none.
pub fn postpone(commitment: &Note, planned_for: Date) -> Result<Note, CommitmentError> {
    open(commitment)?;
    Ok(Note {
        planned_for: Some(planned_for),
        ..commitment.clone()
    })
}

fn close(commitment: &Note, status: CommitmentStatus, now: &Now) -> Result<Note, CommitmentError> {
    open(commitment)?;
    Ok(Note {
        commitment_status: Some(status),
        closed_at: Some(now.timestamp()),
        ..commitment.clone()
    })
}

/// Only a `todo` commitment can change: `done` and `dropped` are final.
fn open(note: &Note) -> Result<(), CommitmentError> {
    match note.commitment_status {
        None => Err(CommitmentError::NotACommitment { id: note.id }),
        Some(CommitmentStatus::Todo) => Ok(()),
        Some(status) => Err(CommitmentError::AlreadyClosed {
            id: note.id,
            status,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::note::{Area, NoteStatus, NoteType};

    const NOW: &str = "2026-10-07T09:00:00+02:00";

    fn now() -> Now {
        Now::parse(NOW).unwrap()
    }

    fn date(text: &str) -> Date {
        text.parse().unwrap()
    }

    fn commitment() -> Note {
        Note {
            id: 12,
            title: "Send retry numbers to Anna".into(),
            body: "Promised on the daily.".into(),
            area: Area::Work,
            note_type: Some(NoteType::Commitment),
            project: None,
            status: NoteStatus::Active,
            commitment_status: Some(CommitmentStatus::Todo),
            source: None,
            repos: vec![],
            tickets: vec![],
            people: vec![],
            expires_on: None,
            owner: None,
            planned_for: Some(date("2026-10-07")),
            closed_at: None,
            created_at: "2026-10-05T09:00:00+02:00".into(),
            updated_at: "2026-10-05T09:00:00+02:00".into(),
        }
    }

    fn done() -> Note {
        mark_done(&commitment(), &now()).unwrap()
    }

    fn dropped() -> Note {
        mark_dropped(&commitment(), &now()).unwrap()
    }

    #[test]
    fn done_sets_status_and_closing_time() {
        assert_eq!(
            done(),
            Note {
                commitment_status: Some(CommitmentStatus::Done),
                closed_at: Some(NOW.into()),
                ..commitment()
            }
        );
        assert_eq!(
            dropped(),
            Note {
                commitment_status: Some(CommitmentStatus::Dropped),
                closed_at: Some(NOW.into()),
                ..commitment()
            }
        );
    }

    #[test]
    fn done_commitment_cannot_be_postponed() {
        assert_eq!(
            postpone(&done(), date("2026-10-09")).unwrap_err(),
            CommitmentError::AlreadyClosed {
                id: 12,
                status: CommitmentStatus::Done
            }
        );
    }

    #[test]
    fn dropped_commitment_cannot_be_postponed() {
        let error = postpone(&dropped(), date("2026-10-09")).unwrap_err();
        assert_eq!(error.to_string(), "commitment #12 is already dropped");
    }

    #[test]
    fn dropped_commitment_cannot_be_reopened() {
        // There is no reopen; and the other closing commands are refused too.
        assert_eq!(
            mark_done(&dropped(), &now()).unwrap_err(),
            CommitmentError::AlreadyClosed {
                id: 12,
                status: CommitmentStatus::Dropped
            }
        );
        assert!(mark_dropped(&dropped(), &now()).is_err());
    }

    #[test]
    fn done_commitment_cannot_be_dropped() {
        assert_eq!(
            mark_dropped(&done(), &now()).unwrap_err(),
            CommitmentError::AlreadyClosed {
                id: 12,
                status: CommitmentStatus::Done
            }
        );
        assert!(mark_done(&done(), &now()).is_err());
    }

    #[test]
    fn postpone_changes_planned_date_only() {
        let postponed = postpone(&commitment(), date("2026-10-09")).unwrap();

        assert_eq!(
            postponed,
            Note {
                planned_for: Some(date("2026-10-09")),
                ..commitment()
            }
        );
    }

    #[test]
    fn postpone_sets_first_date_on_undated_commitment() {
        let undated = Note {
            planned_for: None,
            ..commitment()
        };

        let planned = postpone(&undated, date("2026-10-09")).unwrap();

        assert_eq!(planned.planned_for, Some(date("2026-10-09")));
    }

    #[test]
    fn note_that_is_not_a_commitment_cannot_be_done_dropped_or_postponed() {
        let fact = Note {
            note_type: Some(NoteType::Fact),
            commitment_status: None,
            planned_for: None,
            ..commitment()
        };
        let not_a_commitment = CommitmentError::NotACommitment { id: 12 };

        assert_eq!(mark_done(&fact, &now()).unwrap_err(), not_a_commitment);
        assert_eq!(mark_dropped(&fact, &now()).unwrap_err(), not_a_commitment);
        assert_eq!(
            postpone(&fact, date("2026-10-09")).unwrap_err(),
            not_a_commitment
        );
        assert_eq!(not_a_commitment.to_string(), "note #12 is not a commitment");
    }
}

use super::*;

fn fields(title: &str, body: &str) -> NoteFields {
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

fn note() -> Note {
    Note {
        id: 42,
        title: "Retry 5 times".into(),
        body: "We use 5 retries.".into(),
        area: Area::Work,
        note_type: Some(NoteType::Decision),
        project: None,
        status: NoteStatus::Active,
        commitment_status: None,
        source: None,
        repos: vec!["billing-api".into()],
        tickets: vec![],
        people: vec![],
        expires_on: None,
        owner: None,
        planned_for: None,
        closed_at: None,
        replaced_by: None,
        replaces: vec![],
        related: vec![],
        created_at: "2026-10-06T09:00:00+02:00".into(),
        updated_at: "2026-10-06T09:00:00+02:00".into(),
    }
}

#[test]
fn note_requires_title_body_and_area() {
    assert_eq!(
        NoteDraft::new(fields("", "body")).unwrap_err(),
        NoteError::MissingTitle
    );
    assert_eq!(
        NoteDraft::new(fields("title", "")).unwrap_err(),
        NoteError::MissingBody
    );
    // The area cannot be left out: `NoteFields` has no default for it.
    let draft = NoteDraft::new(fields("title", "body")).unwrap();
    assert_eq!(draft.area, Area::Work);
}

#[test]
fn blank_title_or_body_is_rejected() {
    assert_eq!(
        NoteDraft::new(fields("  \t", "body")).unwrap_err(),
        NoteError::MissingTitle
    );
    assert_eq!(
        NoteDraft::new(fields("title", " \n\n")).unwrap_err(),
        NoteError::MissingBody
    );
}

#[test]
fn note_type_is_optional() {
    let draft = NoteDraft::new(fields("title", "body")).unwrap();
    assert_eq!(draft.note_type, None);
}

#[test]
fn unknown_area_or_type_is_rejected() {
    let error = "hobby".parse::<Area>().unwrap_err();
    assert_eq!(
        error.to_string(),
        "unknown area 'hobby': use work, learning, personal"
    );
    assert!("todo".parse::<NoteType>().is_err());
    assert_eq!("learning".parse::<Area>().unwrap(), Area::Learning);
}

#[test]
fn how_to_type_is_written_with_a_dash() {
    assert_eq!("how-to".parse::<NoteType>().unwrap(), NoteType::HowTo);
    assert_eq!(NoteType::HowTo.to_string(), "how-to");
    assert_eq!(
        serde_json::to_string(&NoteType::HowTo).unwrap(),
        "\"how-to\""
    );
}

#[test]
fn source_needs_kind_and_reference_together() {
    assert_eq!(Source::from_parts(None, None).unwrap(), None);
    assert_eq!(
        Source::from_parts(Some(SourceKind::Ticket), Some("PAY-1234".into())).unwrap(),
        Some(Source {
            kind: SourceKind::Ticket,
            reference: "PAY-1234".into()
        })
    );
    assert_eq!(
        Source::from_parts(Some(SourceKind::Ticket), None).unwrap_err(),
        NoteError::IncompleteSource
    );
    assert_eq!(
        Source::from_parts(None, Some("PAY-1234".into())).unwrap_err(),
        NoteError::IncompleteSource
    );
    assert_eq!(
        Source::from_parts(Some(SourceKind::Ticket), Some("  ".into())).unwrap_err(),
        NoteError::IncompleteSource
    );
}

#[test]
fn new_commitment_starts_as_todo() {
    let mut commitment = fields("Send the report to Anna", "Promised on the daily.");
    commitment.note_type = Some(NoteType::Commitment);
    let commitment = NoteDraft::new(commitment).unwrap();
    assert_eq!(commitment.commitment_status(), Some(CommitmentStatus::Todo));

    let plain = NoteDraft::new(fields("title", "body")).unwrap();
    assert_eq!(plain.commitment_status(), None);
}

#[test]
fn draft_refuses_secret_in_title_body_project_or_source() {
    let token = "ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8";
    let in_title = fields(token, "body");
    let in_body = fields("title", &format!("the token is {token}"));
    let mut in_project = fields("title", "body");
    in_project.project = Some(token.into());
    let mut in_source = fields("title", "body");
    in_source.source = Some(Source {
        kind: SourceKind::Chat,
        reference: token.into(),
    });

    for (field, fields) in [
        ("title", in_title),
        ("body", in_body),
        ("project", in_project),
        ("source reference", in_source),
    ] {
        let error = NoteDraft::new(fields).unwrap_err();
        assert_eq!(
            error,
            NoteError::LooksLikeSecret {
                field,
                kind: SecretKind::GithubToken
            }
        );
        assert!(!error.to_string().contains(token));
    }
}

#[test]
fn draft_trims_text_and_removes_duplicate_repos_and_tickets() {
    let mut fields = fields("  Retry 5 times \n", "\n  indented first line\nsecond\n\n");
    fields.repos = vec![
        "billing-api".into(),
        " billing-api ".into(),
        "web".into(),
        "".into(),
    ];
    fields.tickets = vec!["PAY-1".into(), "PAY-1".into()];

    let draft = NoteDraft::new(fields).unwrap();

    assert_eq!(draft.title, "Retry 5 times");
    assert_eq!(draft.body, "  indented first line\nsecond");
    assert_eq!(draft.repos, ["billing-api", "web"]);
    assert_eq!(draft.tickets, ["PAY-1"]);
}

#[test]
fn edit_changes_only_given_fields() {
    let changes = NoteChanges {
        title: Some("Retry 5 times for billing-api".into()),
        tickets: Some(vec!["PAY-1234".into()]),
        ..NoteChanges::default()
    };

    let edited = note().edited(&changes).unwrap();

    assert_eq!(
        edited,
        Note {
            title: "Retry 5 times for billing-api".into(),
            tickets: vec!["PAY-1234".into()],
            ..note()
        }
    );
}

#[test]
fn edit_with_no_changes_is_rejected() {
    assert_eq!(
        note().edited(&NoteChanges::default()).unwrap_err(),
        NoteError::NothingToChange
    );
}

#[test]
fn edit_follows_the_same_rules_as_a_new_note() {
    let blank_title = NoteChanges {
        title: Some(" ".into()),
        ..NoteChanges::default()
    };
    assert_eq!(
        note().edited(&blank_title).unwrap_err(),
        NoteError::MissingTitle
    );

    let secret_body = NoteChanges {
        body: Some("password=hunter2".into()),
        ..NoteChanges::default()
    };
    assert_eq!(
        note().edited(&secret_body).unwrap_err(),
        NoteError::LooksLikeSecret {
            field: "body",
            kind: SecretKind::PasswordAssignment
        }
    );
}

#[test]
fn changing_type_to_commitment_starts_as_todo_and_back_clears_it() {
    let to_commitment = NoteChanges {
        note_type: Some(NoteType::Commitment),
        ..NoteChanges::default()
    };
    let commitment = note().edited(&to_commitment).unwrap();
    assert_eq!(commitment.commitment_status, Some(CommitmentStatus::Todo));

    let to_fact = NoteChanges {
        note_type: Some(NoteType::Fact),
        ..NoteChanges::default()
    };
    let fact = commitment.edited(&to_fact).unwrap();
    assert_eq!(fact.commitment_status, None);
}

#[test]
fn note_json_uses_the_words_from_the_design() {
    let json = serde_json::to_value(note()).unwrap();
    assert_eq!(json["type"], "decision");
    assert_eq!(json["area"], "work");
    assert_eq!(json["status"], "active");
    assert_eq!(json["commitment_status"], serde_json::Value::Null);
    assert_eq!(serde_json::from_value::<Note>(json).unwrap(), note());
}

fn commitment(status: CommitmentStatus) -> Note {
    Note {
        note_type: Some(NoteType::Commitment),
        commitment_status: Some(status),
        owner: Some(7),
        planned_for: Some("2026-10-08".parse().unwrap()),
        ..note()
    }
}

#[test]
fn only_commitments_have_owner_and_planned_date() {
    let mut with_owner = fields("title", "body");
    with_owner.owner = Some(7);
    assert_eq!(
        NoteDraft::new(with_owner.clone()).unwrap_err(),
        NoteError::OnlyCommitments { field: "an owner" }
    );
    let mut with_date = fields("title", "body");
    with_date.note_type = Some(NoteType::Fact);
    with_date.planned_for = Some("2026-10-08".parse().unwrap());
    assert_eq!(
        NoteDraft::new(with_date).unwrap_err(),
        NoteError::OnlyCommitments {
            field: "a planned date"
        }
    );

    with_owner.note_type = Some(NoteType::Commitment);
    with_owner.planned_for = Some("2026-10-08".parse().unwrap());
    assert!(NoteDraft::new(with_owner).is_ok());

    let owner_on_a_decision = NoteChanges {
        owner: Some(7),
        ..NoteChanges::default()
    };
    assert_eq!(
        note().edited(&owner_on_a_decision).unwrap_err(),
        NoteError::OnlyCommitments { field: "an owner" }
    );
}

#[test]
fn closed_commitment_keeps_its_type() {
    let to_fact = NoteChanges {
        note_type: Some(NoteType::Fact),
        ..NoteChanges::default()
    };

    for status in [CommitmentStatus::Done, CommitmentStatus::Dropped] {
        assert_eq!(
            commitment(status).edited(&to_fact).unwrap_err(),
            NoteError::ClosedCommitmentKeepsType { status }
        );
    }
    assert_eq!(
        NoteError::ClosedCommitmentKeepsType {
            status: CommitmentStatus::Done
        }
        .to_string(),
        "a done commitment keeps its type"
    );
}

#[test]
fn closed_commitment_can_still_get_a_better_title() {
    let better = NoteChanges {
        title: Some("Send the Q3 report".into()),
        ..NoteChanges::default()
    };
    // Naming the type it already has is not a change.
    let same_type = NoteChanges {
        note_type: Some(NoteType::Commitment),
        ..NoteChanges::default()
    };
    let done = commitment(CommitmentStatus::Done);

    let edited = done.edited(&better).unwrap();

    assert_eq!(
        edited,
        Note {
            title: "Send the Q3 report".into(),
            ..done.clone()
        }
    );
    assert_eq!(done.edited(&same_type).unwrap(), done);
}

#[test]
fn changing_type_away_from_commitment_clears_owner_and_planned_date() {
    let to_fact = NoteChanges {
        note_type: Some(NoteType::Fact),
        ..NoteChanges::default()
    };

    let fact = commitment(CommitmentStatus::Todo).edited(&to_fact).unwrap();

    assert_eq!(fact.commitment_status, None);
    assert_eq!(fact.owner, None);
    assert_eq!(fact.planned_for, None);
}

#[test]
fn edit_can_change_the_owner_of_a_commitment() {
    let to_me = NoteChanges {
        owner: Some(9),
        ..NoteChanges::default()
    };

    let edited = commitment(CommitmentStatus::Todo).edited(&to_me).unwrap();

    assert_eq!(edited.owner, Some(9));
    assert_eq!(edited.planned_for, Some("2026-10-08".parse().unwrap()));
}

#[test]
fn duplicate_people_on_a_note_are_kept_once() {
    let mut fields = fields("title", "body");
    fields.people = vec![7, 9, 7];

    assert_eq!(NoteDraft::new(fields).unwrap().people, [7, 9]);

    let changes = NoteChanges {
        people: Some(vec![9, 9]),
        ..NoteChanges::default()
    };
    assert_eq!(note().edited(&changes).unwrap().people, [9]);
}

// ----- links -----

#[test]
fn outdated_note_requires_replaced_by_link() {
    let outdated_without_link = Note {
        status: NoteStatus::Outdated,
        ..note()
    };
    assert_eq!(
        outdated_without_link.check_links().unwrap_err(),
        NoteError::OutdatedWithoutLink { id: 42 }
    );
    // And the other way round: a note that was replaced is outdated.
    let active_with_link = Note {
        replaced_by: Some(43),
        ..note()
    };
    assert!(active_with_link.check_links().is_err());

    assert!(note().check_links().is_ok());
    let replaced = note().replaced_by_note(43).unwrap();
    assert_eq!(
        replaced,
        Note {
            status: NoteStatus::Outdated,
            replaced_by: Some(43),
            ..note()
        }
    );
    assert!(replaced.check_links().is_ok());
}

#[test]
fn note_can_be_replaced_only_once() {
    let replaced = note().replaced_by_note(43).unwrap();

    let error = replaced.replaced_by_note(44).unwrap_err();

    assert_eq!(error, NoteError::AlreadyOutdated { id: 42, by: 43 });
    assert_eq!(
        error.to_string(),
        "note #42 is already outdated, replaced by #43"
    );
}

#[test]
fn note_cannot_replace_or_link_to_itself() {
    assert_eq!(
        note().replaced_by_note(42).unwrap_err(),
        NoteError::SelfLink
    );
    assert_eq!(note().linked_to(42).unwrap_err(), NoteError::SelfLink);
    assert_eq!(
        NoteError::SelfLink.to_string(),
        "a note can never be linked to itself"
    );
    let linked_to_itself = Note {
        related: vec![42],
        ..note()
    };
    assert_eq!(
        linked_to_itself.check_links().unwrap_err(),
        NoteError::SelfLink
    );
}

#[test]
fn linking_twice_changes_nothing() {
    let once = note().linked_to(5).unwrap();
    assert_eq!(once.related, [5]);

    let twice = once.linked_to(5).unwrap();
    assert_eq!(twice, once);

    let more = twice.linked_to(9).unwrap().linked_to(3).unwrap();
    assert_eq!(more.related, [3, 5, 9]);
}

#[test]
fn edit_keeps_links() {
    let linked = note().linked_to(5).unwrap().replaced_by_note(43).unwrap();
    let changes = NoteChanges {
        title: Some("Typo fixed".into()),
        ..NoteChanges::default()
    };

    let edited = linked.edited(&changes).unwrap();

    assert_eq!(edited.related, [5]);
    assert_eq!(edited.replaced_by, Some(43));
    assert_eq!(edited.status, NoteStatus::Outdated);
}

// ----- edit: planned date -----

#[test]
fn edit_planned_moves_date_of_todo_commitment() {
    let later = NoteChanges {
        planned_for: Some("2026-10-12".parse().unwrap()),
        ..NoteChanges::default()
    };

    let edited = commitment(CommitmentStatus::Todo).edited(&later).unwrap();

    assert_eq!(edited.planned_for, Some("2026-10-12".parse().unwrap()));
    assert_eq!(edited.owner, Some(7));
}

#[test]
fn edit_planned_on_done_commitment_or_fact_is_refused() {
    let later = NoteChanges {
        planned_for: Some("2026-10-12".parse().unwrap()),
        ..NoteChanges::default()
    };

    assert_eq!(
        commitment(CommitmentStatus::Done)
            .edited(&later)
            .unwrap_err(),
        NoteError::ClosedCommitmentKeepsDate {
            status: CommitmentStatus::Done
        }
    );
    assert_eq!(
        note().edited(&later).unwrap_err(),
        NoteError::OnlyCommitments {
            field: "a planned date"
        }
    );
}

// ----- edit: add to and remove from the lists -----

fn note_with_lists() -> Note {
    Note {
        repos: vec!["billing-api".into(), "gateway".into()],
        tickets: vec!["PAY-1234".into(), "PAY-1300".into()],
        people: vec![7, 9],
        ..note()
    }
}

#[test]
fn edit_adds_repo_and_keeps_the_others() {
    let changes = NoteChanges {
        add_repos: vec!["ledger".into()],
        add_tickets: vec!["PAY-1400".into()],
        add_people: vec![12],
        ..NoteChanges::default()
    };

    let edited = note_with_lists().edited(&changes).unwrap();

    assert_eq!(edited.repos, ["billing-api", "gateway", "ledger"]);
    assert_eq!(edited.tickets, ["PAY-1234", "PAY-1300", "PAY-1400"]);
    assert_eq!(edited.people, [7, 9, 12]);
}

#[test]
fn edit_removes_ticket() {
    let changes = NoteChanges {
        remove_repos: vec!["gateway".into()],
        remove_tickets: vec!["PAY-1234".into()],
        remove_people: vec![7],
        ..NoteChanges::default()
    };

    let edited = note_with_lists().edited(&changes).unwrap();

    assert_eq!(edited.repos, ["billing-api"]);
    assert_eq!(edited.tickets, ["PAY-1300"]);
    assert_eq!(edited.people, [9]);
}

#[test]
fn edit_add_of_existing_value_changes_nothing() {
    let changes = NoteChanges {
        add_repos: vec!["gateway".into()],
        add_people: vec![9],
        ..NoteChanges::default()
    };

    assert_eq!(
        note_with_lists().edited(&changes).unwrap(),
        note_with_lists()
    );
}

#[test]
fn edit_remove_of_missing_value_is_refused() {
    let wrong_repo = NoteChanges {
        remove_repos: vec!["biling-api".into()],
        ..NoteChanges::default()
    };
    let wrong_person = NoteChanges {
        remove_people: vec![12],
        ..NoteChanges::default()
    };

    let error = note_with_lists().edited(&wrong_repo).unwrap_err();
    assert_eq!(
        error,
        NoteError::NotOnNote {
            id: 42,
            what: "repo",
            value: "biling-api".into()
        }
    );
    assert_eq!(error.to_string(), "note #42 has no repo biling-api");
    assert_eq!(
        note_with_lists()
            .edited(&wrong_person)
            .unwrap_err()
            .to_string(),
        "note #42 has no person #12"
    );
}

#[test]
fn edit_repo_still_replaces_whole_list() {
    let changes = NoteChanges {
        repos: Some(vec!["ledger".into()]),
        ..NoteChanges::default()
    };

    let edited = note_with_lists().edited(&changes).unwrap();

    assert_eq!(edited.repos, ["ledger"]);
    assert_eq!(edited.tickets, ["PAY-1234", "PAY-1300"]);
}

// ----- replace: what the newer note takes over -----

fn sourced_note() -> Note {
    Note {
        project: Some("Billing".into()),
        source: Some(Source {
            kind: SourceKind::Meeting,
            reference: "Sprint planning".into(),
        }),
        expires_on: Some("2026-12-31".parse().unwrap()),
        ..note_with_lists()
    }
}

#[test]
fn replacement_copies_area_type_project_repos_tickets_people_source() {
    let newer = sourced_note().replacement(Replacement::new("Retry 7 times", "We use 7 retries."));

    assert_eq!(newer.title, "Retry 7 times");
    assert_eq!(newer.body, "We use 7 retries.");
    assert_eq!(newer.area, Area::Work);
    assert_eq!(newer.note_type, Some(NoteType::Decision));
    assert_eq!(newer.project.as_deref(), Some("Billing"));
    assert_eq!(newer.repos, ["billing-api", "gateway"]);
    assert_eq!(newer.tickets, ["PAY-1234", "PAY-1300"]);
    assert_eq!(newer.people, [7, 9]);
    assert_eq!(newer.source, sourced_note().source);
}

#[test]
fn replacement_flag_overrides_copied_field() {
    let newer = sourced_note().replacement(Replacement {
        area: Some(Area::Learning),
        note_type: Some(NoteType::Fact),
        project: Some("Payments".into()),
        repos: Some(vec!["ledger".into()]),
        tickets: Some(vec!["PAY-1500".into()]),
        people: Some(vec![12]),
        source: Some(Source {
            kind: SourceKind::Chat,
            reference: "Teams".into(),
        }),
        ..Replacement::new("Retry 7 times", "We use 7 retries.")
    });

    assert_eq!(newer.area, Area::Learning);
    assert_eq!(newer.note_type, Some(NoteType::Fact));
    assert_eq!(newer.project.as_deref(), Some("Payments"));
    assert_eq!(newer.repos, ["ledger"]);
    assert_eq!(newer.tickets, ["PAY-1500"]);
    assert_eq!(newer.people, [12]);
    assert_eq!(newer.source.unwrap().kind, SourceKind::Chat);
}

#[test]
fn replacement_of_commitment_keeps_owner_and_planned_date() {
    let old = commitment(CommitmentStatus::Todo);

    let same = old.replacement(Replacement::new("Review the PR", "Body."));
    let moved = old.replacement(Replacement {
        owner: Some(9),
        planned_for: Some("2026-10-12".parse().unwrap()),
        ..Replacement::new("Review the PR", "Body.")
    });

    assert_eq!(same.owner, Some(7));
    assert_eq!(same.planned_for, Some("2026-10-08".parse().unwrap()));
    assert_eq!(moved.owner, Some(9));
    assert_eq!(moved.planned_for, Some("2026-10-12".parse().unwrap()));
}

#[test]
fn replacement_with_other_type_drops_owner_and_planned_date() {
    let newer = commitment(CommitmentStatus::Todo).replacement(Replacement {
        note_type: Some(NoteType::Fact),
        ..Replacement::new("The PR was reviewed", "Body.")
    });

    assert_eq!(newer.owner, None);
    assert_eq!(newer.planned_for, None);
    assert!(NoteDraft::new(newer).is_ok());
}

#[test]
fn replacement_does_not_copy_expiry() {
    let copied = sourced_note().replacement(Replacement::new("Title", "Body."));
    let given = sourced_note().replacement(Replacement {
        expires_on: Some("2027-01-31".parse().unwrap()),
        ..Replacement::new("Title", "Body.")
    });

    assert_eq!(copied.expires_on, None);
    assert_eq!(given.expires_on, Some("2027-01-31".parse().unwrap()));
}

// ----- a weekday must match its date -----

#[test]
fn title_and_body_refuse_a_weekday_that_does_not_match_its_date() {
    // 2026-10-10 is a Saturday.
    let mut in_body = fields("Review the PR", "Anna reviews the PR by Friday 2026-10-10.");
    let error = NoteDraft::new(in_body.clone()).unwrap_err();
    assert_eq!(
        error,
        NoteError::WrongWeekday {
            field: "body",
            said: "Friday 2026-10-10".into(),
            date: "2026-10-10".into(),
            actual: "Saturday",
        }
    );
    assert_eq!(
        error.to_string(),
        "the body says \"Friday 2026-10-10\", but 2026-10-10 is a Saturday: check the date \
         (`nv date` lists the next days)"
    );

    in_body.title = "Review by Fri 2026-10-10".into();
    in_body.body = "Anna reviews the PR.".into();
    assert!(matches!(
        NoteDraft::new(in_body).unwrap_err(),
        NoteError::WrongWeekday { field: "title", .. }
    ));
    assert!(NoteDraft::new(fields("Review", "By Friday 2026-10-09.")).is_ok());
}

#[test]
fn edit_refuses_a_wrong_weekday_in_the_body_that_stays_and_accepts_the_fix() {
    let legacy = Note {
        body: "Anna reviews the PR by Friday 2026-10-10.".into(),
        ..commitment(CommitmentStatus::Todo)
    };
    let only_the_date = NoteChanges {
        planned_for: Some("2026-10-09".parse().unwrap()),
        ..NoteChanges::default()
    };
    let date_and_body = NoteChanges {
        body: Some("Anna reviews the PR by Friday 2026-10-09.".into()),
        ..only_the_date.clone()
    };

    assert!(matches!(
        legacy.edited(&only_the_date).unwrap_err(),
        NoteError::WrongWeekday { field: "body", .. }
    ));
    let fixed = legacy.edited(&date_and_body).unwrap();
    assert_eq!(fixed.planned_for, Some("2026-10-09".parse().unwrap()));
}

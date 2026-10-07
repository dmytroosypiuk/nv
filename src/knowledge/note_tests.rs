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
        source: None,
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
fn new_commitment_note_starts_as_todo() {
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

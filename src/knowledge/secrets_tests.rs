use super::*;

#[test]
fn refuses_github_token() {
    let text = "Use ghp_a1B2c3D4e5F6g7H8i9J0k1L2m3N4o5P6q7R8 for the CI";
    assert_eq!(find_secret(text), Some(SecretKind::GithubToken));
    assert_eq!(
        find_secret("github_pat_11ABCDEFG0abcdefghijkl_mnopqrstuvwxyz0123456789"),
        Some(SecretKind::GithubToken)
    );
}

#[test]
fn refuses_sk_key() {
    assert_eq!(
        find_secret("key is sk-proj-abcdefghijklmnopqrstuvwx"),
        Some(SecretKind::ApiKey)
    );
}

#[test]
fn refuses_password_assignment() {
    for text in [
        "password=hunter2",
        "DB_PASSWORD = 'hunter2'",
        "api_key=abc123",
        "token: a8f3k2j9x1",
        "\"secret\": \"s3cr3tvalue\"",
    ] {
        assert_eq!(
            find_secret(text),
            Some(SecretKind::PasswordAssignment),
            "{text}"
        );
    }
}

#[test]
fn refuses_private_key_block() {
    assert_eq!(
        find_secret("-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1r"),
        Some(SecretKind::PrivateKey)
    );
}

#[test]
fn refuses_long_random_string() {
    assert_eq!(
        find_secret("header: Zx9Qw3Er7Ty1Ui5Op2As8Df4Gh6Jk0LzXc"),
        Some(SecretKind::LongRandomString)
    );
}

#[test]
fn allows_commit_sha_uuid_url_and_file_path() {
    for text in [
        "fixed in 9fceb02d0ae598e95dc970b74767f19372d61af8",
        "request id 123e4567-e89b-12d3-a456-426614174000",
        "see https://dev.azure.com/Contoso/Billing/_workitems/edit/123456?view=Details2026",
        "file src/Knowledge/Store/NoteRepositoryImplementation2/Handler.rs",
        "test check_rejects_owner_or_planned_date_on_non_commitment_note",
    ] {
        assert_eq!(find_secret(text), None, "{text}");
    }
}

#[test]
fn allows_the_word_password_in_plain_text() {
    for text in [
        "Ask DevOps in #infra-help for the staging password.",
        "The token expires after 15 minutes.",
        "Password: ask Anna",
        "Run with password=$DB_PASSWORD from the environment",
        "Set token=<your token> in the config",
    ] {
        assert_eq!(find_secret(text), None, "{text}");
    }
}

#[test]
fn error_does_not_repeat_the_secret() {
    let kind = find_secret("password=hunter2").unwrap();
    assert!(!kind.to_string().contains("hunter2"));
    assert!(kind.to_string().contains("password"), "{kind}");
}

//! Safety net: nv never stores text that looks like a secret.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;

/// Why a text looks like a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKind {
    GithubToken,
    ApiKey,
    PasswordAssignment,
    PrivateKey,
    LongRandomString,
}

impl fmt::Display for SecretKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::GithubToken => "a GitHub token",
            Self::ApiKey => "an API key (sk-…)",
            Self::PasswordAssignment => "a password or token assignment (like password=…)",
            Self::PrivateKey => "a private key",
            Self::LongRandomString => "a long random string",
        })
    }
}

static PRIVATE_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap());
static GITHUB_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,})").unwrap()
});
static API_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\bsk-[A-Za-z0-9_-]{20,}").unwrap());
static ASSIGNMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?i)(?:password|passwd|pwd|secret|token|api[_-]?key|access[_-]?key)["']?\s*([=:])\s*["']?([^\s"']+)"#,
    )
    .unwrap()
});
static LONG_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[A-Za-z0-9_+-]{32,}").unwrap());

/// The first thing in `text` that looks like a secret, if any.
pub fn find_secret(text: &str) -> Option<SecretKind> {
    if PRIVATE_KEY.is_match(text) {
        Some(SecretKind::PrivateKey)
    } else if GITHUB_TOKEN.is_match(text) {
        Some(SecretKind::GithubToken)
    } else if API_KEY.is_match(text) {
        Some(SecretKind::ApiKey)
    } else if ASSIGNMENT
        .captures_iter(text)
        .any(|found| assigns_a_real_value(&found[1], &found[2]))
    {
        Some(SecretKind::PasswordAssignment)
    } else if LONG_WORD
        .find_iter(text)
        .any(|word| looks_random(word.as_str()))
    {
        Some(SecretKind::LongRandomString)
    } else {
        None
    }
}

/// `password=hunter2` is a secret; `password=$DB_PASSWORD` and "Password: ask Anna" are not.
fn assigns_a_real_value(separator: &str, value: &str) -> bool {
    let is_placeholder = value.starts_with(['$', '<', '{', '%'])
        || value.chars().all(|c| matches!(c, '*' | 'x' | 'X' | '.'));
    if is_placeholder {
        return false;
    }
    // After a colon plain words are normal prose, so ask for something password-like.
    separator == "="
        || (value.len() >= 8
            && value.chars().any(|c| c.is_ascii_digit())
            && value.chars().any(|c| c.is_ascii_alphabetic()))
}

/// Mixed case with several digits. Hex-only strings (commit SHAs, UUIDs) are fine.
fn looks_random(word: &str) -> bool {
    word.chars().any(|c| c.is_ascii_lowercase())
        && word.chars().any(|c| c.is_ascii_uppercase())
        && word.chars().filter(char::is_ascii_digit).count() >= 3
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;

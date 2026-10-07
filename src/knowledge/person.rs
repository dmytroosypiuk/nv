//! The Person aggregate: one main name, many aliases, always addressed by ID.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Person {
    pub id: i64,
    /// The main name.
    pub name: String,
    pub role: Option<String>,
    /// Other forms of the name: "Anna K.", an email, a Teams name.
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PersonError {
    #[error("a person needs a name")]
    MissingName,
    #[error("an alias cannot be empty")]
    BlankAlias,
    #[error("nothing to change: give a name or a role")]
    NothingToChange,
    #[error("person #{id} not found")]
    NotFound { id: i64 },
    #[error("'{name}' is already person #{id} {owner}")]
    NameTaken {
        name: String,
        id: i64,
        owner: String,
    },
    #[error("alias '{alias}' already belongs to person #{id} {owner}")]
    AliasTaken {
        alias: String,
        id: i64,
        owner: String,
    },
    #[error("a person cannot be merged into itself")]
    SamePerson,
}

/// A full alias ("Anna Nowak", "Anna N.", an email) belongs to one person.
/// A short one ("Anna") can match several people.
pub fn is_full_alias(alias: &str) -> bool {
    alias.split_whitespace().count() > 1 || alias.contains('@')
}

/// A name or alias without spaces around it and with single spaces inside.
pub fn tidy(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Names are compared without case: "anna nowak" is "Anna Nowak".
pub fn same_name(a: &str, b: &str) -> bool {
    a.to_lowercase() == b.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alias_with_two_words_or_an_at_sign_is_full() {
        for alias in [
            "Anna Nowak",
            "Anna N.",
            "anna.nowak@contoso.com",
            "@anna",
            "Nowak, Anna",
        ] {
            assert!(is_full_alias(alias), "{alias}");
        }
    }

    #[test]
    fn one_word_alias_is_short() {
        for alias in ["Anna", "Ania", "anowak", " Anna "] {
            assert!(!is_full_alias(alias), "{alias}");
        }
    }

    #[test]
    fn names_are_tidied_and_compared_without_case() {
        assert_eq!(tidy("  Anna   Nowak \n"), "Anna Nowak");
        assert_eq!(tidy(" \t"), "");
        assert!(same_name("anna NOWAK", "Anna Nowak"));
        assert!(same_name("Żaneta", "żaneta"));
        assert!(!same_name("Anna Nowak", "Anna Kowalska"));
    }
}

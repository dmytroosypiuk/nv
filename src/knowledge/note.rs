//! The Note aggregate: what a note is and which changes it accepts.

use std::ops::Deref;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::secrets::{SecretKind, find_secret};

/// A rule of the Note aggregate that a change would break.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NoteError {
    #[error("a note needs a title")]
    MissingTitle,
    #[error("a note needs a body: give it on stdin")]
    MissingBody,
    #[error("a source needs both a kind and a reference")]
    IncompleteSource,
    #[error("nothing to change: give at least one field")]
    NothingToChange,
    #[error(
        "the {field} looks like it holds {kind}. nv never stores secrets: \
         save how to get access instead"
    )]
    LooksLikeSecret {
        field: &'static str,
        kind: SecretKind,
    },
    #[error("unknown {what} '{value}': use {allowed}")]
    UnknownWord {
        what: &'static str,
        value: String,
        allowed: String,
    },
}

/// An enum that is stored, printed and parsed as one fixed word.
macro_rules! word_enum {
    ($(#[$meta:meta])* $name:ident, $what:literal, { $($variant:ident => $word:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub enum $name {
            $(#[serde(rename = $word)] $variant),+
        }

        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $word),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        impl std::str::FromStr for $name {
            type Err = $crate::knowledge::note::NoteError;

            fn from_str(word: &str) -> Result<Self, Self::Err> {
                match word {
                    $($word => Ok(Self::$variant),)+
                    unknown => Err(Self::Err::UnknownWord {
                        what: $what,
                        value: unknown.to_string(),
                        allowed: [$($word),+].join(", "),
                    }),
                }
            }
        }
    };
}
pub(crate) use word_enum;

word_enum!(
    /// What a note is about.
    Area, "area", { Work => "work", Learning => "learning", Personal => "personal" }
);
word_enum!(
    NoteType, "type", {
        Decision => "decision",
        Commitment => "commitment",
        HowTo => "how-to",
        Fact => "fact",
        Idea => "idea",
    }
);
word_enum!(
    /// Where a note came from.
    SourceKind, "source kind", {
        Meeting => "meeting",
        Chat => "chat",
        Email => "email",
        Ticket => "ticket",
        Web => "web",
        Repo => "repo",
        Doc => "doc",
    }
);
word_enum!(
    /// Outdated: was true, a newer note replaced it.
    NoteStatus, "status", { Active => "active", Outdated => "outdated" }
);
word_enum!(
    CommitmentStatus, "commitment status", { Todo => "todo", Done => "done", Dropped => "dropped" }
);

/// Where a note came from: a kind plus a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    pub kind: SourceKind,
    #[serde(rename = "ref")]
    pub reference: String,
}

impl Source {
    /// A source is optional, but never half given.
    pub fn from_parts(
        kind: Option<SourceKind>,
        reference: Option<String>,
    ) -> Result<Option<Self>, NoteError> {
        let reference = reference.map(|reference| reference.trim().to_string());
        match (kind, reference) {
            (None, None) => Ok(None),
            (Some(kind), Some(reference)) if !reference.is_empty() => {
                Ok(Some(Self { kind, reference }))
            }
            _ => Err(NoteError::IncompleteSource),
        }
    }
}

/// The fields Claude or the user gives for a new note, not checked yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteFields {
    pub title: String,
    pub body: String,
    pub area: Area,
    pub note_type: Option<NoteType>,
    pub project: Option<String>,
    pub repos: Vec<String>,
    pub tickets: Vec<String>,
    pub source: Option<Source>,
}

impl NoteFields {
    /// The same fields, tidied, if they follow every rule of a note.
    fn checked(mut self) -> Result<Self, NoteError> {
        self.title = self.title.trim().to_string();
        // Keep the indentation of the first line: bodies can start with code.
        self.body = self
            .body
            .trim_end()
            .trim_start_matches(['\n', '\r'])
            .to_string();
        if self.title.is_empty() {
            return Err(NoteError::MissingTitle);
        }
        if self.body.trim().is_empty() {
            return Err(NoteError::MissingBody);
        }
        self.repos = without_blanks_and_duplicates(self.repos);
        self.tickets = without_blanks_and_duplicates(self.tickets);

        let source_reference = self.source.as_ref().map(|source| source.reference.as_str());
        let texts = [
            ("title", Some(self.title.as_str())),
            ("body", Some(self.body.as_str())),
            ("project", self.project.as_deref()),
            ("source reference", source_reference),
        ];
        for (field, text) in texts {
            if let Some(kind) = text.and_then(find_secret) {
                return Err(NoteError::LooksLikeSecret { field, kind });
            }
        }
        Ok(self)
    }
}

fn without_blanks_and_duplicates(words: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::new();
    for word in words {
        let word = word.trim();
        if !word.is_empty() && !kept.iter().any(|seen| seen == word) {
            kept.push(word.to_string());
        }
    }
    kept
}

/// A new note that passed every rule and can be saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteDraft(NoteFields);

impl NoteDraft {
    pub fn new(fields: NoteFields) -> Result<Self, NoteError> {
        fields.checked().map(Self)
    }

    /// Only commitments have a status, and they start as todo.
    pub fn commitment_status(&self) -> Option<CommitmentStatus> {
        (self.note_type == Some(NoteType::Commitment)).then_some(CommitmentStatus::Todo)
    }
}

impl Deref for NoteDraft {
    type Target = NoteFields;

    fn deref(&self) -> &NoteFields {
        &self.0
    }
}

/// The fields to change in an edit; `None` keeps the old value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NoteChanges {
    pub title: Option<String>,
    pub body: Option<String>,
    pub area: Option<Area>,
    pub note_type: Option<NoteType>,
    pub project: Option<String>,
    pub repos: Option<Vec<String>>,
    pub tickets: Option<Vec<String>>,
    pub source: Option<Source>,
}

/// A saved note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: i64,
    pub title: String,
    pub body: String,
    pub area: Area,
    #[serde(rename = "type")]
    pub note_type: Option<NoteType>,
    pub project: Option<String>,
    pub status: NoteStatus,
    pub commitment_status: Option<CommitmentStatus>,
    pub source: Option<Source>,
    pub repos: Vec<String>,
    pub tickets: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl Note {
    /// This note with `changes` applied, if the result still follows every rule.
    pub fn edited(&self, changes: &NoteChanges) -> Result<Note, NoteError> {
        if *changes == NoteChanges::default() {
            return Err(NoteError::NothingToChange);
        }
        let changes = changes.clone();
        let fields = NoteFields {
            title: changes.title.unwrap_or_else(|| self.title.clone()),
            body: changes.body.unwrap_or_else(|| self.body.clone()),
            area: changes.area.unwrap_or(self.area),
            note_type: changes.note_type.or(self.note_type),
            project: changes.project.or_else(|| self.project.clone()),
            repos: changes.repos.unwrap_or_else(|| self.repos.clone()),
            tickets: changes.tickets.unwrap_or_else(|| self.tickets.clone()),
            source: changes.source.or_else(|| self.source.clone()),
        }
        .checked()?;

        let commitment_status = match fields.note_type {
            Some(NoteType::Commitment) => self.commitment_status.or(Some(CommitmentStatus::Todo)),
            _ => None,
        };
        Ok(Note {
            id: self.id,
            title: fields.title,
            body: fields.body,
            area: fields.area,
            note_type: fields.note_type,
            project: fields.project,
            status: self.status,
            commitment_status,
            source: fields.source,
            repos: fields.repos,
            tickets: fields.tickets,
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        })
    }
}

#[cfg(test)]
#[path = "note_tests.rs"]
mod tests;

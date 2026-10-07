//! The Note aggregate: what a note is and which changes it accepts.

use std::ops::Deref;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::secrets::{SecretKind, find_secret};
use super::weekday_check::find_wrong_weekday;
use crate::clock::Date;

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
    #[error("a note can never be linked to itself")]
    SelfLink,
    #[error("note #{id} is already outdated, replaced by #{by}")]
    AlreadyOutdated { id: i64, by: i64 },
    #[error("note #{id} is outdated but no note replaces it")]
    OutdatedWithoutLink { id: i64 },
    #[error("only a commitment has {field}: use --type commitment")]
    OnlyCommitments { field: &'static str },
    #[error("a {status} commitment keeps its type")]
    ClosedCommitmentKeepsType { status: CommitmentStatus },
    #[error("a {status} commitment keeps its planned date")]
    ClosedCommitmentKeepsDate { status: CommitmentStatus },
    #[error("note #{id} has no {what} {value}")]
    NotOnNote {
        id: i64,
        what: &'static str,
        value: String,
    },
    #[error(
        "the {field} looks like it holds {kind}. nv never stores secrets: \
         save how to get access instead"
    )]
    LooksLikeSecret {
        field: &'static str,
        kind: SecretKind,
    },
    #[error(
        "the {field} says \"{said}\", but {date} is a {actual}: check the date \
         (`nv date` lists the next days)"
    )]
    WrongWeekday {
        field: &'static str,
        said: String,
        date: String,
        actual: &'static str,
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
    /// IDs of the people who were involved.
    pub people: Vec<i64>,
    pub source: Option<Source>,
    /// For time-limited facts: the last day the note is true.
    pub expires_on: Option<Date>,
    /// Commitments only: the person who promised. `None` = me.
    pub owner: Option<i64>,
    /// Commitments only.
    pub planned_for: Option<Date>,
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
        if self.note_type != Some(NoteType::Commitment) {
            if self.owner.is_some() {
                return Err(NoteError::OnlyCommitments { field: "an owner" });
            }
            if self.planned_for.is_some() {
                return Err(NoteError::OnlyCommitments {
                    field: "a planned date",
                });
            }
        }
        self.repos = without_blanks_and_duplicates(self.repos);
        self.tickets = without_blanks_and_duplicates(self.tickets);
        let mut people = Vec::new();
        for person in self.people {
            if !people.contains(&person) {
                people.push(person);
            }
        }
        self.people = people;

        for (field, text) in [("title", &self.title), ("body", &self.body)] {
            if let Some(wrong) = find_wrong_weekday(text) {
                return Err(NoteError::WrongWeekday {
                    field,
                    said: wrong.said,
                    date: wrong.date.to_string(),
                    actual: wrong.actual,
                });
            }
        }
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
    pub people: Option<Vec<i64>>,
    pub source: Option<Source>,
    pub expires_on: Option<Date>,
    pub owner: Option<i64>,
    /// Commitments that are still todo only.
    pub planned_for: Option<Date>,
    /// Added to the list; a value that is already there changes nothing.
    pub add_repos: Vec<String>,
    pub add_tickets: Vec<String>,
    pub add_people: Vec<i64>,
    /// Taken out of the list; a value that is not there is refused.
    pub remove_repos: Vec<String>,
    pub remove_tickets: Vec<String>,
    pub remove_people: Vec<i64>,
}

/// What a newer note says when it replaces an old one; `None` copies the old value.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replacement {
    pub title: String,
    pub body: String,
    pub area: Option<Area>,
    pub note_type: Option<NoteType>,
    pub project: Option<String>,
    pub repos: Option<Vec<String>>,
    pub tickets: Option<Vec<String>>,
    pub people: Option<Vec<i64>>,
    pub source: Option<Source>,
    /// Never copied: what is true now has its own end.
    pub expires_on: Option<Date>,
    pub owner: Option<i64>,
    pub planned_for: Option<Date>,
}

impl Replacement {
    /// A replacement that copies everything it can from the old note.
    pub fn new(title: &str, body: &str) -> Self {
        Self {
            title: title.into(),
            body: body.into(),
            ..Self::default()
        }
    }
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
    /// IDs of the people who were involved.
    #[serde(default)]
    pub people: Vec<i64>,
    #[serde(default)]
    pub expires_on: Option<Date>,
    /// Commitments only: the person who promised. `None` = me.
    #[serde(default, rename = "owner_person_id")]
    pub owner: Option<i64>,
    #[serde(default)]
    pub planned_for: Option<Date>,
    /// When a commitment was done or dropped.
    #[serde(default)]
    pub closed_at: Option<String>,
    /// The newer note that says what is true now. Set exactly when the note is outdated.
    #[serde(default)]
    pub replaced_by: Option<i64>,
    /// The older notes this note replaced.
    #[serde(default)]
    pub replaces: Vec<i64>,
    /// Related notes; the link has no direction.
    #[serde(default)]
    pub related: Vec<i64>,
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
        let note_type = changes.note_type.or(self.note_type);
        if note_type != self.note_type
            && let Some(status @ (CommitmentStatus::Done | CommitmentStatus::Dropped)) =
                self.commitment_status
        {
            // Otherwise changing the type and back would reopen it.
            return Err(NoteError::ClosedCommitmentKeepsType { status });
        }
        if changes.planned_for.is_some()
            && let Some(status @ (CommitmentStatus::Done | CommitmentStatus::Dropped)) =
                self.commitment_status
        {
            return Err(NoteError::ClosedCommitmentKeepsDate { status });
        }
        // A note that stops being a commitment loses what only commitments have.
        let stays_commitment = note_type == Some(NoteType::Commitment);
        let not_on_note = |what, value: String| NoteError::NotOnNote {
            id: self.id,
            what,
            value,
        };
        let repos = changed_list(
            changes.repos.unwrap_or_else(|| self.repos.clone()),
            changes.add_repos,
            &changes.remove_repos,
        )
        .map_err(|repo| not_on_note("repo", repo))?;
        let tickets = changed_list(
            changes.tickets.unwrap_or_else(|| self.tickets.clone()),
            changes.add_tickets,
            &changes.remove_tickets,
        )
        .map_err(|ticket| not_on_note("ticket", ticket))?;
        let people = changed_list(
            changes.people.unwrap_or_else(|| self.people.clone()),
            changes.add_people,
            &changes.remove_people,
        )
        .map_err(|person| not_on_note("person", format!("#{person}")))?;
        let fields = NoteFields {
            title: changes.title.unwrap_or_else(|| self.title.clone()),
            body: changes.body.unwrap_or_else(|| self.body.clone()),
            area: changes.area.unwrap_or(self.area),
            note_type,
            project: changes.project.or_else(|| self.project.clone()),
            repos,
            tickets,
            people,
            source: changes.source.or_else(|| self.source.clone()),
            expires_on: changes.expires_on.or(self.expires_on),
            owner: changes.owner.or(self.owner.filter(|_| stays_commitment)),
            planned_for: changes
                .planned_for
                .or(self.planned_for.filter(|_| stays_commitment)),
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
            people: fields.people,
            expires_on: fields.expires_on,
            owner: fields.owner,
            planned_for: fields.planned_for,
            closed_at: self.closed_at.clone().filter(|_| stays_commitment),
            replaced_by: self.replaced_by,
            replaces: self.replaces.clone(),
            related: self.related.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        })
    }
}

/// The list without `remove` and with `add` at the end. Gives back the first value of
/// `remove` that is not in the list.
fn changed_list<T: PartialEq + Clone>(
    mut list: Vec<T>,
    add: Vec<T>,
    remove: &[T],
) -> Result<Vec<T>, T> {
    for value in remove {
        let Some(position) = list.iter().position(|kept| kept == value) else {
            return Err(value.clone());
        };
        list.remove(position);
    }
    for value in add {
        if !list.contains(&value) {
            list.push(value);
        }
    }
    Ok(list)
}

impl Note {
    /// The fields of the newer note that replaces this one: area, type, project, repos,
    /// tickets, people and source are copied unless `newer` gives its own. A commitment
    /// that stays a commitment keeps its owner and planned date too.
    pub fn replacement(&self, newer: Replacement) -> NoteFields {
        let note_type = newer.note_type.or(self.note_type);
        let stays_commitment = note_type == Some(NoteType::Commitment);
        NoteFields {
            title: newer.title,
            body: newer.body,
            area: newer.area.unwrap_or(self.area),
            note_type,
            project: newer.project.or_else(|| self.project.clone()),
            repos: newer.repos.unwrap_or_else(|| self.repos.clone()),
            tickets: newer.tickets.unwrap_or_else(|| self.tickets.clone()),
            people: newer.people.unwrap_or_else(|| self.people.clone()),
            source: newer.source.or_else(|| self.source.clone()),
            expires_on: newer.expires_on,
            owner: newer.owner.or(self.owner.filter(|_| stays_commitment)),
            planned_for: newer
                .planned_for
                .or(self.planned_for.filter(|_| stays_commitment)),
        }
    }

    /// This note marked outdated, with the link to the newer note that replaces it.
    pub fn replaced_by_note(&self, newer_id: i64) -> Result<Note, NoteError> {
        if newer_id == self.id {
            return Err(NoteError::SelfLink);
        }
        if let Some(by) = self.replaced_by {
            return Err(NoteError::AlreadyOutdated { id: self.id, by });
        }
        Ok(Note {
            status: NoteStatus::Outdated,
            replaced_by: Some(newer_id),
            ..self.clone()
        })
    }

    /// This note with a `related` link to another note. Linking twice changes nothing.
    pub fn linked_to(&self, other_id: i64) -> Result<Note, NoteError> {
        if other_id == self.id {
            return Err(NoteError::SelfLink);
        }
        let mut linked = self.clone();
        if !linked.related.contains(&other_id) {
            linked.related.push(other_id);
            linked.related.sort_unstable();
        }
        Ok(linked)
    }

    /// An outdated note always has a `replaced_by` link, an active note never has one,
    /// and no link points to the note itself.
    pub fn check_links(&self) -> Result<(), NoteError> {
        if (self.status == NoteStatus::Outdated) != self.replaced_by.is_some() {
            return Err(NoteError::OutdatedWithoutLink { id: self.id });
        }
        if self.replaced_by == Some(self.id) || self.related.contains(&self.id) {
            return Err(NoteError::SelfLink);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "note_tests.rs"]
mod tests;

//! How results look: compact text by default, JSON with `--json`.

use std::io::Write;

use anyhow::Result;
use serde::Serialize;

use crate::commitments::today::TodayView;
use crate::knowledge::history::HistoryEntry;
use crate::knowledge::note::{Note, NoteStatus};
use crate::knowledge::person::Person;

pub fn json(out: &mut dyn Write, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer_pretty(&mut *out, value)?;
    writeln!(out)?;
    Ok(())
}

/// The answer to add, edit and delete: `Saved #42`, or the note as JSON.
pub fn change(out: &mut dyn Write, what_happened: &str, note: &Note, as_json: bool) -> Result<()> {
    if as_json {
        json(out, note)
    } else {
        writeln!(out, "{what_happened} #{}", note.id)?;
        Ok(())
    }
}

/// A note plus the names the text output shows instead of IDs.
pub struct ShownNote {
    pub note: Note,
    pub owner_name: Option<String>,
    pub people_names: Vec<String>,
}

pub fn full_note(out: &mut dyn Write, shown: &ShownNote) -> Result<()> {
    write!(out, "{}", summary(shown))?;
    writeln!(out, "\n{}", shown.note.body)?;
    Ok(())
}

pub fn found_notes(out: &mut dyn Write, notes: &[ShownNote]) -> Result<()> {
    if notes.is_empty() {
        writeln!(out, "No notes found.")?;
    }
    let summaries: Vec<String> = notes.iter().map(summary).collect();
    write!(out, "{}", summaries.join("\n"))?;
    Ok(())
}

pub fn found_notes_json(out: &mut dyn Write, notes: &[ShownNote]) -> Result<()> {
    #[derive(Serialize)]
    struct Found<'a> {
        rank: usize,
        #[serde(flatten)]
        note: &'a Note,
    }
    let found: Vec<Found> = notes
        .iter()
        .enumerate()
        .map(|(index, shown)| Found {
            rank: index + 1,
            note: &shown.note,
        })
        .collect();
    json(out, &found)
}

/// Two lists: what I planned for today (overdue included), and what others owe me.
pub fn today(out: &mut dyn Write, view: &TodayView) -> Result<()> {
    if view.is_empty() {
        writeln!(out, "Nothing planned for today.")?;
        return Ok(());
    }
    let ids = view
        .mine
        .iter()
        .chain(view.owed.iter().map(|owed| &owed.note));
    let id_width = ids.map(|note| note.id.to_string().len()).max().unwrap_or(1);
    let line = |note: &Note, text: String| {
        let planned = match note.planned_for {
            Some(planned) if planned < view.today => format!(" · planned {planned}, overdue"),
            Some(planned) if planned > view.today => format!(" · planned {planned}"),
            _ => String::new(),
        };
        format!("#{:<id_width$}  {text}{planned}\n", note.id)
    };

    let mut sections = Vec::new();
    if !view.mine.is_empty() {
        let mut section = format!("Planned for today ({})\n", view.today);
        for note in &view.mine {
            section.push_str(&line(note, note.title.clone()));
        }
        sections.push(section);
    }
    if !view.owed.is_empty() {
        let mut section = String::from("Others owe you\n");
        for owed in &view.owed {
            section.push_str(&line(
                &owed.note,
                format!("{}: {}", owed.owner_name, owed.note.title),
            ));
        }
        sections.push(section);
    }
    if view.undated > 0 {
        sections.push(format!(
            "{} more of yours have no date: nv search --type commitment\n",
            view.undated
        ));
    }
    write!(out, "{}", sections.join("\n"))?;
    Ok(())
}

/// The change log, newest first, one change per line.
pub fn history(out: &mut dyn Write, entries: &[HistoryEntry]) -> Result<()> {
    if entries.is_empty() {
        writeln!(out, "No changes yet.")?;
    }
    let width = |text_of: &dyn Fn(&HistoryEntry) -> String| {
        entries
            .iter()
            .map(|entry| text_of(entry).len())
            .max()
            .unwrap_or(0)
    };
    let id_width = width(&|entry| entry.id.to_string());
    let actor_width = width(&|entry| entry.actor.to_string());
    let action_width = width(&|entry| entry.action.to_string());
    for entry in entries {
        // "2026-10-07T09:12:00+02:00" is shown as "2026-10-07 09:12".
        let at = entry.at.get(..16).unwrap_or(&entry.at).replace('T', " ");
        let title = entry.title.as_deref().unwrap_or("");
        let undone = if entry.undone { "  (undone)" } else { "" };
        writeln!(
            out,
            "#{:<id_width$}  {at}  {:<actor_width$}  {:<action_width$}  {} #{}  {title}{undone}",
            entry.id,
            entry.actor.as_str(),
            entry.action.as_str(),
            entry.subject,
            entry.subject_id,
        )?;
    }
    Ok(())
}

/// Every person with role and all aliases, so Claude sees which Anna is which.
pub fn people(out: &mut dyn Write, people: &[Person], as_json: bool) -> Result<()> {
    if as_json {
        return json(out, &people);
    }
    if people.is_empty() {
        writeln!(out, "No people found.")?;
    }
    let id_width = people
        .iter()
        .map(|person| person.id.to_string().len())
        .max()
        .unwrap_or(1);
    for person in people {
        let role = person
            .role
            .as_ref()
            .map_or(String::new(), |role| format!(" · {role}"));
        writeln!(out, "#{:<id_width$}  {}{role}", person.id, person.name)?;
        if !person.aliases.is_empty() {
            let indent = " ".repeat(id_width + 3);
            writeln!(out, "{indent}aliases: {}", person.aliases.join(", "))?;
        }
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct ModelInfo<'a> {
    pub model: &'a str,
    pub dims: usize,
    pub folder: String,
    pub installed: bool,
    pub embedded: usize,
    pub pending: usize,
}

pub fn model_info(out: &mut dyn Write, info: &ModelInfo<'_>) -> Result<()> {
    let found = if info.installed { "found" } else { "missing" };
    writeln!(out, "model: {} ({} dimensions)", info.model, info.dims)?;
    writeln!(out, "folder: {} ({found})", info.folder)?;
    writeln!(
        out,
        "notes: {} embedded, {} pending",
        info.embedded, info.pending
    )?;
    Ok(())
}

pub fn embedded(out: &mut dyn Write, notes: usize) -> Result<()> {
    writeln!(out, "Embedded {notes} notes")?;
    Ok(())
}

/// Two or three lines: ID, type, area, date and status; the title; then the details.
fn summary(shown: &ShownNote) -> String {
    let note = &shown.note;
    let id = format!("#{}  ", note.id);
    let indent = " ".repeat(id.len());
    let note_type = note
        .note_type
        .map_or("note", |note_type| note_type.as_str());
    let date = note.created_at.get(..10).unwrap_or(&note.created_at);
    // An outdated note points to the newer note that says what is true now.
    let newer = note
        .replaced_by
        .map_or(String::new(), |newer| format!(" → #{newer}"));
    let mut text = format!(
        "{id}{note_type} · {} · {date} · {}{newer}\n{indent}{}\n",
        note.area,
        status_word(note),
        note.title
    );
    let details = details(shown);
    if !details.is_empty() {
        text.push_str(&format!("{indent}{}\n", details.join(" · ")));
    }
    text
}

/// A commitment shows where it stands (todo, done, dropped) unless it is outdated.
fn status_word(note: &Note) -> &'static str {
    match (note.status, note.commitment_status) {
        (NoteStatus::Active, Some(commitment_status)) => commitment_status.as_str(),
        (status, _) => status.as_str(),
    }
}

fn details(shown: &ShownNote) -> Vec<String> {
    let note = &shown.note;
    let mut details = Vec::new();
    if let Some(owner_name) = &shown.owner_name {
        details.push(format!("owner: {owner_name}"));
    }
    if let Some(planned_for) = note.planned_for {
        details.push(format!("planned: {planned_for}"));
    }
    if let Some(project) = &note.project {
        details.push(format!("project: {project}"));
    }
    if !note.repos.is_empty() {
        details.push(format!("repos: {}", note.repos.join(", ")));
    }
    if !note.tickets.is_empty() {
        details.push(format!("tickets: {}", note.tickets.join(", ")));
    }
    if !shown.people_names.is_empty() {
        details.push(format!("people: {}", shown.people_names.join(", ")));
    }
    if let Some(source) = &note.source {
        details.push(format!("source: {}, {}", source.kind, source.reference));
    }
    if let Some(expires_on) = note.expires_on {
        details.push(format!("expires: {expires_on}"));
    }
    if !note.replaces.is_empty() {
        details.push(format!("replaces: {}", note_ids(&note.replaces)));
    }
    if !note.related.is_empty() {
        details.push(format!("related: {}", note_ids(&note.related)));
    }
    details
}

fn note_ids(ids: &[i64]) -> String {
    let ids: Vec<String> = ids.iter().map(|id| format!("#{id}")).collect();
    ids.join(", ")
}

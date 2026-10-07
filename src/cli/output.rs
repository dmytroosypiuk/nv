//! How results look: compact text by default, JSON with `--json`.

use std::io::Write;

use anyhow::Result;
use serde::Serialize;

use crate::clock::Date;
use crate::commitments::today::TodayView;
use crate::knowledge::history::HistoryEntry;
use crate::knowledge::note::{Note, NoteStatus};
use crate::knowledge::person::Person;

pub fn json(out: &mut dyn Write, value: &impl Serialize) -> Result<()> {
    serde_json::to_writer_pretty(&mut *out, value)?;
    writeln!(out)?;
    Ok(())
}

/// The answer to add, edit, replace and delete: `Saved #42`, or the note as JSON.
pub fn change(out: &mut dyn Write, what_happened: &str, note: &Note, as_json: bool) -> Result<()> {
    if as_json {
        json(out, note)
    } else {
        writeln!(out, "{what_happened}")?;
        Ok(())
    }
}

/// `, planned Thu 2026-10-08, expires Sat 2026-11-14`: the dates of a saved note with
/// their weekdays. Claude repeats them to the user, who sees a wrong day at once.
pub fn dates(note: &Note) -> String {
    let mut dates = String::new();
    if let Some(planned_for) = note.planned_for {
        dates.push_str(&format!(", planned {}", planned_for.with_weekday()));
    }
    if let Some(expires_on) = note.expires_on {
        dates.push_str(&format!(", expires {}", expires_on.with_weekday()));
    }
    dates
}

/// A note plus the names the text output shows instead of IDs.
pub struct ShownNote {
    pub note: Note,
    pub owner_name: Option<String>,
    pub people_names: Vec<String>,
}

pub fn full_note(out: &mut dyn Write, shown: &ShownNote) -> Result<()> {
    write!(out, "{}", summary(shown, false))?;
    writeln!(out, "\n{}", shown.note.body)?;
    Ok(())
}

/// The found notes, each with the start of its body. `total` is how many notes a
/// filter-only search found: when the limit cut the list, the last line says so.
pub fn found_notes(out: &mut dyn Write, notes: &[ShownNote], total: Option<usize>) -> Result<()> {
    if notes.is_empty() {
        writeln!(out, "No notes found.")?;
    }
    let summaries: Vec<String> = notes.iter().map(|shown| summary(shown, true)).collect();
    write!(out, "{}", summaries.join("\n"))?;
    if let Some(total) = total
        && total > notes.len()
    {
        writeln!(out, "\nshowing {} of {total}, use --limit", notes.len())?;
    }
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

/// One day of `nv date`.
#[derive(Serialize)]
pub struct CalendarDay {
    date: String,
    weekday: &'static str,
    label: Option<&'static str>,
}

/// `today` and the `days` after it.
pub fn calendar(today: Date, days: u16) -> Vec<CalendarDay> {
    (0..=i64::from(days))
        .map_while(|offset| Some((offset, today.plus_days(offset)?)))
        .map(|(offset, date)| CalendarDay {
            date: date.to_string(),
            weekday: date.weekday_name(),
            label: match offset {
                0 => Some("today"),
                1 => Some("tomorrow"),
                _ => None,
            },
        })
        .collect()
}

pub fn calendar_text(out: &mut dyn Write, days: &[CalendarDay]) -> Result<()> {
    for day in days {
        let weekday = day.weekday.get(..3).unwrap_or(day.weekday);
        match day.label {
            Some(label) => writeln!(out, "{weekday} {}  {label}", day.date)?,
            None => writeln!(out, "{weekday} {}", day.date)?,
        }
    }
    Ok(())
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
            "{} more of yours have no date: nv search --type commitment --status todo\n",
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

/// How much of the first body line a search result shows.
const FIRST_LINE_LENGTH: usize = 100;

/// Two to four lines: ID, type, area, date and status; the title; the start of the body
/// when `with_first_line`; then the details.
fn summary(shown: &ShownNote, with_first_line: bool) -> String {
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
    if with_first_line && let Some(line) = first_line(&note.body) {
        text.push_str(&format!("{indent}{line}\n"));
    }
    let details = details(shown);
    if !details.is_empty() {
        text.push_str(&format!("{indent}{}\n", details.join(" · ")));
    }
    text
}

/// The first paragraph of the body as one line, cut when it is long. Bodies are often
/// wrapped, so one line of the body would stop in the middle of a sentence.
fn first_line(body: &str) -> Option<String> {
    let paragraph: Vec<&str> = body
        .lines()
        .map(str::trim)
        .skip_while(|line| line.is_empty())
        .take_while(|line| !line.is_empty())
        .collect();
    let line = paragraph.join(" ");
    if line.is_empty() {
        return None;
    }
    if line.chars().count() <= FIRST_LINE_LENGTH {
        return Some(line);
    }
    let cut: String = line.chars().take(FIRST_LINE_LENGTH).collect();
    Some(format!("{}…", cut.trim_end()))
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

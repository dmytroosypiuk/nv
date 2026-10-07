//! Command line: object first, then action.

mod background;
mod output;

pub use background::{BackgroundEmbedding, DetachedEmbedder};

use std::fs::File;
use std::io::{Read, Write};

use anyhow::{Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};

use crate::clock::{Date, Now};
use crate::commitments::today::today_view;
use crate::commitments::{mark_done, mark_dropped, postpone};
use crate::config::NvHome;
use crate::db;
use crate::knowledge::change_log::{Action, Actor};
use crate::knowledge::history::{HistoryFilter, history, undo};
use crate::knowledge::note::{
    Area, Note, NoteChanges, NoteDraft, NoteFields, NoteType, Source, SourceKind,
};
use crate::knowledge::people::{PeopleStore, person_name};
use crate::knowledge::store::NoteStore;
use crate::search::embedder::ModelLoader;
use crate::search::filter::NoteFilter;
use crate::search::vectors;
use crate::search::{SearchRequest, search};

/// A local, offline knowledge store for Claude Code.
#[derive(Debug, Parser)]
#[command(name = "nv", version, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Save, read, change and find notes
    #[command(subcommand)]
    Note(NoteCommand),
    /// Shortcut for `nv note add`
    Add(AddArgs),
    /// Shortcut for `nv note search`
    Search(SearchArgs),
    /// Promises: done, drop, postpone and the today view
    #[command(subcommand)]
    Commitment(CommitmentCommand),
    /// Shortcut for `nv commitment today`
    Today(TodayArgs),
    /// People: main name, aliases, role. Always addressed by ID
    #[command(subcommand)]
    People(PeopleCommand),
    /// The change log; `nv history undo` takes a change back
    #[command(args_conflicts_with_subcommands = true)]
    History {
        #[command(subcommand)]
        command: Option<HistoryCommand>,
        #[command(flatten)]
        args: HistoryArgs,
    },
    /// The embedding model and its index
    #[command(subcommand)]
    Model(ModelCommand),
}

#[derive(Debug, Subcommand)]
enum HistoryCommand {
    /// Undo one change; without an ID, the newest one that still stands
    Undo { change_id: Option<i64> },
}

#[derive(Debug, Args)]
struct HistoryArgs {
    /// Only changes of this note
    #[arg(long)]
    note: Option<i64>,
    /// Only changes of this person
    #[arg(long)]
    person: Option<i64>,
    /// How many changes to show at most
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum PeopleCommand {
    /// Add a person with their main name
    Add {
        name: String,
        #[arg(long)]
        role: Option<String>,
        /// Another form of the name; can be given many times
        #[arg(long = "alias")]
        aliases: Vec<String>,
    },
    /// Change the main name or the role
    Edit {
        id: i64,
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// Show everyone
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show every person whose name or alias contains the text
    Search {
        text: String,
        #[arg(long)]
        json: bool,
    },
    /// Add an alias to a person
    Alias { id: i64, alias: String },
    /// Make one person out of two: the second one moves into the first
    Merge { keep_id: i64, other_id: i64 },
}

#[derive(Debug, Subcommand)]
enum CommitmentCommand {
    /// What you planned for today, and what others owe you
    Today(TodayArgs),
    /// The promise was kept
    Done { id: i64 },
    /// The promise will not be kept
    Drop { id: i64 },
    /// Move the planned date, like 2026-10-09
    Postpone { id: i64, date: Date },
}

#[derive(Debug, Args)]
struct TodayArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum ModelCommand {
    /// Show the model, its folder and how many notes are embedded
    Info(ModelInfoArgs),
    /// Embed every note again
    Reindex,
    /// Embed the notes that have no embedding yet (started by `nv add`)
    #[command(hide = true)]
    EmbedPending,
}

#[derive(Debug, Args)]
struct ModelInfoArgs {
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum NoteCommand {
    /// Save a new note; the body is read from stdin
    Add(AddArgs),
    /// Show one note in full
    Show(ShowArgs),
    /// Change fields of a note that is still true
    Edit(EditArgs),
    /// Save a newer note that says what is true now; the old one becomes outdated
    Replace(ReplaceArgs),
    /// Link two notes as related
    Link { id: i64, other_id: i64 },
    /// Delete a note that was a mistake (kept in the change log)
    Delete(DeleteArgs),
    /// Find notes by meaning, keyword and filters
    Search(SearchArgs),
}

#[derive(Debug, Args)]
struct AddArgs {
    /// Short, with the words you would search for later
    #[arg(long)]
    title: String,
    /// work, learning or personal
    #[arg(long)]
    area: Area,
    /// decision, commitment, how-to, fact or idea
    #[arg(long = "type")]
    note_type: Option<NoteType>,
    #[arg(long)]
    project: Option<String>,
    /// Can be given many times
    #[arg(long = "repo")]
    repos: Vec<String>,
    /// Can be given many times
    #[arg(long = "ticket")]
    tickets: Vec<String>,
    /// ID of a person who was involved; can be given many times
    #[arg(long = "person")]
    people: Vec<i64>,
    /// meeting, chat, email, ticket, web, repo or doc
    #[arg(long, requires = "source_ref")]
    source_kind: Option<SourceKind>,
    /// Which meeting, ticket, URL or path
    #[arg(long, requires = "source_kind")]
    source_ref: Option<String>,
    /// For time-limited facts: the last day the note is true, like 2026-10-12
    #[arg(long)]
    expires_on: Option<Date>,
    /// Commitments: ID of the person who promised. Leave out when it is you
    #[arg(long)]
    owner: Option<i64>,
    /// Commitments: the day it is planned for, like 2026-10-08
    #[arg(long)]
    planned_for: Option<Date>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct ReplaceArgs {
    /// The note that is no longer true
    old_id: i64,
    #[command(flatten)]
    note: AddArgs,
}

#[derive(Debug, Args)]
struct ShowArgs {
    id: i64,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct EditArgs {
    id: i64,
    #[arg(long)]
    title: Option<String>,
    /// Read the new body from stdin
    #[arg(long)]
    body: bool,
    #[arg(long)]
    area: Option<Area>,
    #[arg(long = "type")]
    note_type: Option<NoteType>,
    #[arg(long)]
    project: Option<String>,
    /// Replaces all repos of the note
    #[arg(long = "repo")]
    repos: Option<Vec<String>>,
    /// Replaces all tickets of the note
    #[arg(long = "ticket")]
    tickets: Option<Vec<String>>,
    /// Replaces all people of the note
    #[arg(long = "person")]
    people: Option<Vec<i64>>,
    #[arg(long, requires = "source_ref")]
    source_kind: Option<SourceKind>,
    #[arg(long, requires = "source_kind")]
    source_ref: Option<String>,
    #[arg(long)]
    expires_on: Option<Date>,
    /// Commitments: ID of the person who promised
    #[arg(long)]
    owner: Option<i64>,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct DeleteArgs {
    id: i64,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
#[command(group(
    clap::ArgGroup::new("what")
        .required(true)
        .multiple(true)
        .args(["query", "area", "note_type", "repo", "ticket", "person", "since", "planned"])
))]
struct SearchArgs {
    /// What to look for, in English. Can be left out when a filter is given
    query: Option<String>,
    #[arg(long)]
    area: Option<Area>,
    #[arg(long = "type")]
    note_type: Option<NoteType>,
    #[arg(long)]
    repo: Option<String>,
    #[arg(long)]
    ticket: Option<String>,
    /// ID of a person: notes about them and commitments they own
    #[arg(long)]
    person: Option<i64>,
    /// Notes created on or after this day, like 2026-10-01
    #[arg(long)]
    since: Option<Date>,
    /// Commitments planned for this day, like 2026-10-08
    #[arg(long)]
    planned: Option<Date>,
    /// Include expired notes
    #[arg(long)]
    all: bool,
    /// How many notes to show at most
    #[arg(long, default_value_t = 5)]
    limit: usize,
    #[arg(long)]
    json: bool,
}

/// Everything a command needs from the outside world, read once in `main`.
pub struct Context<'a> {
    pub nv_home: NvHome,
    pub actor: Actor,
    pub now: Now,
    /// Loads the embedding model, only when a command needs vectors.
    pub loader: &'a mut dyn ModelLoader,
    /// Starts `nv model embed-pending` without waiting for it.
    pub background: &'a mut dyn BackgroundEmbedding,
}

/// Where a command reads the body from and writes its answer and warnings to.
pub struct Streams<'a> {
    pub stdin: &'a mut dyn Read,
    pub out: &'a mut dyn Write,
    pub err: &'a mut dyn Write,
}

/// Runs one parsed command.
pub fn run(cli: Cli, context: &mut Context<'_>, streams: Streams<'_>) -> Result<()> {
    let conn = db::open(&context.nv_home.db_path())?;
    let store = NoteStore::new(&conn);
    let (actor, now) = (context.actor, context.now);
    let model_dir = context.nv_home.model_dir();
    let Streams { stdin, out, err } = streams;

    match cli.command {
        Command::Add(args) | Command::Note(NoteCommand::Add(args)) => {
            let as_json = args.json;
            let note = store.add(&draft(args, stdin)?, actor, &now)?;
            embed_in_background(context);
            output::change(out, "Saved", &note, as_json)
        }
        Command::Note(NoteCommand::Replace(args)) => {
            let as_json = args.note.json;
            let note = store.replace(args.old_id, &draft(args.note, stdin)?, actor, &now)?;
            embed_in_background(context);
            if as_json {
                output::json(out, &note)
            } else {
                writeln!(out, "Saved #{}, replaces #{}", note.id, args.old_id)?;
                Ok(())
            }
        }
        Command::Note(NoteCommand::Link { id, other_id }) => {
            if store.link(id, other_id, actor, &now)? {
                writeln!(out, "Linked #{id} and #{other_id}")?;
            } else {
                writeln!(out, "#{id} and #{other_id} are already linked")?;
            }
            Ok(())
        }
        Command::Note(NoteCommand::Show(args)) => {
            let note = store
                .get(args.id)?
                .ok_or_else(|| anyhow!("note #{} not found", args.id))?;
            if args.json {
                output::json(out, &note)
            } else {
                output::full_note(out, &shown(&conn, note)?)
            }
        }
        Command::Note(NoteCommand::Edit(args)) => {
            let changes = NoteChanges {
                title: args.title,
                body: args.body.then(|| read_body(stdin)).transpose()?,
                area: args.area,
                note_type: args.note_type,
                project: args.project,
                repos: args.repos,
                tickets: args.tickets,
                people: args.people,
                source: Source::from_parts(args.source_kind, args.source_ref)?,
                expires_on: args.expires_on,
                owner: args.owner,
            };
            let note = store.edit(args.id, &changes, actor, &now)?;
            embed_in_background(context);
            output::change(out, "Edited", &note, args.json)
        }
        Command::Note(NoteCommand::Delete(args)) => {
            let note = store.delete(args.id, actor, &now)?;
            output::change(out, "Deleted", &note, args.json)
        }
        Command::Search(args) | Command::Note(NoteCommand::Search(args)) => {
            let request = SearchRequest {
                text: args.query.as_deref(),
                filter: NoteFilter {
                    area: args.area,
                    note_type: args.note_type,
                    repo: args.repo,
                    ticket: args.ticket,
                    person: args.person,
                    since: args.since,
                    planned: args.planned,
                    include_expired: args.all,
                },
                limit: args.limit,
                today: now.today(),
            };
            let outcome = search(&conn, &request, context.loader)?;
            if outcome.model_missing {
                writeln!(
                    err,
                    "nv: model not found in {}: keyword search only",
                    model_dir.display()
                )?;
            }
            let mut found = Vec::new();
            for id in outcome.note_ids {
                if let Some(note) = store.get(id)? {
                    found.push(shown(&conn, note)?);
                }
            }
            if args.json {
                output::found_notes_json(out, &found)
            } else {
                output::found_notes(out, &found)
            }
        }
        Command::Today(args) | Command::Commitment(CommitmentCommand::Today(args)) => {
            let view = today_view(&conn, now.today())?;
            if args.json {
                output::json(out, &view)
            } else {
                output::today(out, &view)
            }
        }
        Command::Commitment(CommitmentCommand::Done { id }) => {
            store.change(id, Action::Done, actor, &now, |note| {
                Ok(mark_done(note, &now)?)
            })?;
            writeln!(out, "Done #{id}")?;
            Ok(())
        }
        Command::Commitment(CommitmentCommand::Drop { id }) => {
            store.change(id, Action::Drop, actor, &now, |note| {
                Ok(mark_dropped(note, &now)?)
            })?;
            writeln!(out, "Dropped #{id}")?;
            Ok(())
        }
        Command::Commitment(CommitmentCommand::Postpone { id, date }) => {
            store.change(id, Action::Postpone, actor, &now, |note| {
                Ok(postpone(note, date)?)
            })?;
            writeln!(out, "Postponed #{id} to {date}")?;
            Ok(())
        }
        Command::History {
            command: None,
            args,
        } => {
            let filter = HistoryFilter {
                note: args.note,
                person: args.person,
            };
            let entries = history(&conn, filter, args.limit)?;
            if args.json {
                output::json(out, &entries)
            } else {
                output::history(out, &entries)
            }
        }
        Command::History {
            command: Some(HistoryCommand::Undo { change_id }),
            ..
        } => {
            let outcome = undo(&conn, change_id, actor, &now)?;
            for warning in &outcome.warnings {
                writeln!(err, "nv: {warning}")?;
            }
            if outcome.text_changed {
                embed_in_background(context);
            }
            writeln!(
                out,
                "Undone #{}: {}",
                outcome.change.id, outcome.description
            )?;
            Ok(())
        }
        Command::People(command) => {
            let people = PeopleStore::new(&conn);
            match command {
                PeopleCommand::Add {
                    name,
                    role,
                    aliases,
                } => {
                    let person = people.add(&name, role.as_deref(), &aliases, actor, &now)?;
                    writeln!(out, "Added person #{}", person.id)?;
                    Ok(())
                }
                PeopleCommand::Edit { id, name, role } => {
                    people.edit(id, name.as_deref(), role.as_deref(), actor, &now)?;
                    writeln!(out, "Edited person #{id}")?;
                    Ok(())
                }
                PeopleCommand::List { json } => output::people(out, &people.list()?, json),
                PeopleCommand::Search { text, json } => {
                    output::people(out, &people.search(&text)?, json)
                }
                PeopleCommand::Alias { id, alias } => {
                    people.add_alias(id, &alias, actor, &now)?;
                    writeln!(out, "Added alias to person #{id}")?;
                    Ok(())
                }
                PeopleCommand::Merge { keep_id, other_id } => {
                    people.merge(keep_id, other_id, actor, &now)?;
                    writeln!(out, "Merged person #{other_id} into #{keep_id}")?;
                    Ok(())
                }
            }
        }
        Command::Model(ModelCommand::Info(args)) => {
            let model = context.loader.model();
            let info = output::ModelInfo {
                model,
                dims: context.loader.dims(),
                folder: model_dir.display().to_string(),
                installed: context.loader.is_installed(),
                embedded: vectors::embedded_count(&conn, model)?,
                pending: vectors::pending_notes(&conn, model)?.len(),
            };
            if args.json {
                output::json(out, &info)
            } else {
                output::model_info(out, &info)
            }
        }
        Command::Model(ModelCommand::Reindex) => {
            let Some(embedder) = context.loader.load()? else {
                bail!(
                    "model not found in {}: copy the model files there",
                    model_dir.display()
                );
            };
            vectors::clear(&conn, embedder.model())?;
            let embedded = vectors::embed_pending(&conn, embedder)?;
            output::embedded(out, embedded)
        }
        Command::Model(ModelCommand::EmbedPending) => {
            // Many `nv add` in a row start many of these, and each would load the model
            // (330 MB): only one runs, and it goes on until nothing is pending.
            let lock = File::create(context.nv_home.root().join("embed.lock"))?;
            if lock.try_lock().is_err() {
                writeln!(out, "Embedding is already running")?;
                return Ok(());
            }
            let mut embedded = 0;
            loop {
                let nothing_pending =
                    vectors::pending_notes(&conn, context.loader.model())?.is_empty();
                // Nothing pending: the model is not loaded at all.
                let Some(embedder) = context.loader.load_unless(nothing_pending)? else {
                    break;
                };
                embedded += vectors::embed_pending(&conn, embedder)?;
            }
            output::embedded(out, embedded)
        }
    }
}

/// Builds the new note of `nv add` and `nv note replace`; the body comes from stdin.
fn draft(args: AddArgs, stdin: &mut dyn Read) -> Result<NoteDraft> {
    Ok(NoteDraft::new(NoteFields {
        title: args.title,
        body: read_body(stdin)?,
        area: args.area,
        note_type: args.note_type,
        project: args.project,
        repos: args.repos,
        tickets: args.tickets,
        people: args.people,
        source: Source::from_parts(args.source_kind, args.source_ref)?,
        expires_on: args.expires_on,
        owner: args.owner,
        planned_for: args.planned_for,
    })?)
}

/// A note with what the text output needs besides the note itself.
fn shown(conn: &rusqlite::Connection, note: Note) -> Result<output::ShownNote> {
    let owner_name = match note.owner {
        Some(owner) => person_name(conn, owner)?,
        None => None,
    };
    let mut people_names = Vec::new();
    for &person in &note.people {
        people_names.extend(person_name(conn, person)?);
    }
    Ok(output::ShownNote {
        note,
        owner_name,
        people_names,
    })
}

/// Starts embedding the saved note without waiting. If this fails, the note is still
/// saved and the next search embeds it.
fn embed_in_background(context: &mut Context<'_>) {
    if context.loader.is_installed() {
        let _ = context.background.start();
    }
}

fn read_body(stdin: &mut dyn Read) -> Result<String> {
    let mut body = String::new();
    stdin.read_to_string(&mut body)?;
    Ok(body)
}

#[cfg(test)]
mod tests;

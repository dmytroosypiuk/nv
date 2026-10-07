//! Command line: object first, then action.

mod background;
mod output;

pub use background::{BackgroundEmbedding, DetachedEmbedder};

use std::fs::File;
use std::io::{Read, Write};

use anyhow::{Result, anyhow, bail};
use clap::{Args, Parser, Subcommand};

use crate::clock::{Date, Now};
use crate::config::NvHome;
use crate::db;
use crate::knowledge::change_log::Actor;
use crate::knowledge::note::{
    Area, NoteChanges, NoteDraft, NoteFields, NoteType, Source, SourceKind,
};
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
    /// The embedding model and its index
    #[command(subcommand)]
    Model(ModelCommand),
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
    /// meeting, chat, email, ticket, web, repo or doc
    #[arg(long, requires = "source_ref")]
    source_kind: Option<SourceKind>,
    /// Which meeting, ticket, URL or path
    #[arg(long, requires = "source_kind")]
    source_ref: Option<String>,
    /// For time-limited facts: the last day the note is true, like 2026-10-12
    #[arg(long)]
    expires_on: Option<Date>,
    #[arg(long)]
    json: bool,
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
    #[arg(long, requires = "source_ref")]
    source_kind: Option<SourceKind>,
    #[arg(long, requires = "source_kind")]
    source_ref: Option<String>,
    #[arg(long)]
    expires_on: Option<Date>,
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
        .args(["query", "area", "note_type", "repo", "ticket", "since", "planned"])
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
            let draft = NoteDraft::new(NoteFields {
                title: args.title,
                body: read_body(stdin)?,
                area: args.area,
                note_type: args.note_type,
                project: args.project,
                repos: args.repos,
                tickets: args.tickets,
                source: Source::from_parts(args.source_kind, args.source_ref)?,
                expires_on: args.expires_on,
            })?;
            let note = store.add(&draft, actor, &now)?;
            embed_in_background(context);
            output::change(out, "Saved", &note, args.json)
        }
        Command::Note(NoteCommand::Show(args)) => {
            let note = store
                .get(args.id)?
                .ok_or_else(|| anyhow!("note #{} not found", args.id))?;
            if args.json {
                output::json(out, &note)
            } else {
                output::full_note(out, &note)
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
                source: Source::from_parts(args.source_kind, args.source_ref)?,
                expires_on: args.expires_on,
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
                found.extend(store.get(id)?);
            }
            if args.json {
                output::found_notes_json(out, &found)
            } else {
                output::found_notes(out, &found)
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

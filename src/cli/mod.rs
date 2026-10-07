//! Command line: object first, then action.

mod output;

use std::io::{Read, Write};

use anyhow::{Result, anyhow};
use clap::{Args, Parser, Subcommand};

use crate::clock::Now;
use crate::config::NvHome;
use crate::db;
use crate::knowledge::change_log::Actor;
use crate::knowledge::note::{
    Area, NoteChanges, NoteDraft, NoteFields, NoteType, Source, SourceKind,
};
use crate::knowledge::store::NoteStore;
use crate::search::keyword::keyword_search;

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
    /// Find notes by keyword
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
    json: bool,
}

#[derive(Debug, Args)]
struct DeleteArgs {
    id: i64,
    #[arg(long)]
    json: bool,
}

#[derive(Debug, Args)]
struct SearchArgs {
    /// Words to look for, in English
    query: String,
    /// How many notes to show at most
    #[arg(long, default_value_t = 5)]
    limit: usize,
    #[arg(long)]
    json: bool,
}

/// Everything a command needs from the outside world, read once in `main`.
pub struct Context {
    pub nv_home: NvHome,
    pub actor: Actor,
    pub now: Now,
}

/// Runs one parsed command. The body of a note comes from `stdin`, results go to `out`.
pub fn run(cli: Cli, context: &Context, stdin: &mut dyn Read, out: &mut dyn Write) -> Result<()> {
    let conn = db::open(&context.nv_home.db_path())?;
    let store = NoteStore::new(&conn);
    let Context { actor, now, .. } = context;

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
            })?;
            let note = store.add(&draft, *actor, now)?;
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
            };
            let note = store.edit(args.id, &changes, *actor, now)?;
            output::change(out, "Edited", &note, args.json)
        }
        Command::Note(NoteCommand::Delete(args)) => {
            let note = store.delete(args.id, *actor, now)?;
            output::change(out, "Deleted", &note, args.json)
        }
        Command::Search(args) | Command::Note(NoteCommand::Search(args)) => {
            let mut found = Vec::new();
            for id in keyword_search(&conn, &args.query, args.limit)? {
                found.extend(store.get(id)?);
            }
            if args.json {
                output::found_notes_json(out, &found)
            } else {
                output::found_notes(out, &found)
            }
        }
    }
}

fn read_body(stdin: &mut dyn Read) -> Result<String> {
    let mut body = String::new();
    stdin.read_to_string(&mut body)?;
    Ok(body)
}

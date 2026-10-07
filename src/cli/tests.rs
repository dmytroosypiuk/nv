//! Commands that need vectors, run in-process with the fake embedder.

use clap::Parser;
use tempfile::TempDir;

use super::*;
use crate::search::embedder::fake::FakeLoader;

#[derive(Default)]
struct RecordedBackground {
    started: usize,
}

impl BackgroundEmbedding for RecordedBackground {
    fn start(&mut self) -> Result<()> {
        self.started += 1;
        Ok(())
    }
}

/// One nv "machine": a home folder, a fake model and a recorded background starter.
struct Nv {
    home: TempDir,
    loader: FakeLoader,
    background: RecordedBackground,
}

struct Answer {
    out: String,
    err: String,
}

impl Nv {
    fn new() -> Self {
        Self {
            home: TempDir::new().unwrap(),
            loader: FakeLoader::default(),
            background: RecordedBackground::default(),
        }
    }

    fn run_at(&mut self, time: &str, args: &[&str], stdin: &str) -> Answer {
        let cli = Cli::try_parse_from([&["nv"], args].concat()).unwrap();
        let mut context = Context {
            nv_home: NvHome::resolve(Some(self.home.path().into()), None).unwrap(),
            actor: Actor::Claude,
            now: Now::parse(time).unwrap(),
            loader: &mut self.loader,
            background: &mut self.background,
        };
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let streams = Streams {
            stdin: &mut stdin.as_bytes(),
            out: &mut out,
            err: &mut err,
        };
        run(cli, &mut context, streams).unwrap();
        Answer {
            out: String::from_utf8(out).unwrap(),
            err: String::from_utf8(err).unwrap(),
        }
    }

    fn run(&mut self, args: &[&str], stdin: &str) -> Answer {
        self.run_at("2026-10-07T09:00:00+02:00", args, stdin)
    }

    fn add(&mut self, title: &str, body: &str, flags: &[&str]) {
        let args = [&["add", "--title", title, "--area", "work"], flags].concat();
        self.run_at("2026-10-05T09:00:00+02:00", &args, body);
    }
}

#[test]
fn add_show_and_filter_only_search_do_not_load_model() {
    let mut nv = Nv::new();

    nv.add(
        "Retry 5 times",
        "For billing calls.",
        &["--ticket", "PAY-1234"],
    );
    nv.run(&["note", "show", "1"], "");
    nv.run(&["note", "edit", "1", "--project", "Billing"], "");
    let found = nv.run(&["search", "--ticket", "PAY-1234"], "");
    nv.run(&["model", "info"], "");

    assert!(
        found
            .out
            .starts_with("#1  note · work · 2026-10-05 · active\n")
    );
    assert_eq!(found.err, "");
    assert_eq!(nv.loader.loads, 0);
}

#[test]
fn add_and_edit_start_background_embedding() {
    let mut nv = Nv::new();

    nv.add("Retry 5 times", "For billing calls.", &[]);
    assert_eq!(nv.background.started, 1);

    nv.run(&["note", "edit", "1", "--body"], "Now 7 retries.");
    assert_eq!(nv.background.started, 2);

    nv.run(&["note", "show", "1"], "");
    nv.run(&["search", "retries"], "");
    assert_eq!(nv.background.started, 2);
}

#[test]
fn background_embedding_is_not_started_without_a_model() {
    let mut nv = Nv::new();
    nv.loader = FakeLoader::missing();

    nv.add("Retry 5 times", "For billing calls.", &[]);

    assert_eq!(nv.background.started, 0);
}

#[test]
fn embed_pending_command_embeds_pending_notes() {
    let mut nv = Nv::new();
    nv.add("Retry 5 times", "For billing calls.", &[]);
    nv.add("Staging postgres", "Bastion host.", &[]);

    let first = nv.run(&["model", "embed-pending"], "");
    let second = nv.run(&["model", "embed-pending"], "");

    assert_eq!(first.out, "Embedded 2 notes\n");
    assert_eq!(second.out, "Embedded 0 notes\n");
    assert_eq!(nv.loader.embedder.embedded.len(), 2);
    // Nothing pending: the model is not even loaded.
    assert_eq!(nv.loader.loads, 1);
}

#[test]
fn reindex_embeds_every_note_again() {
    let mut nv = Nv::new();
    nv.add("Retry 5 times", "For billing calls.", &[]);
    nv.add("Staging postgres", "Bastion host.", &[]);
    nv.run(&["model", "embed-pending"], "");

    let answer = nv.run(&["model", "reindex"], "");

    assert_eq!(answer.out, "Embedded 2 notes\n");
    assert_eq!(nv.loader.embedder.embedded.len(), 4);
}

#[test]
fn model_info_shows_embedded_and_pending_counts_without_loading_model() {
    let mut nv = Nv::new();
    nv.add("Retry 5 times", "For billing calls.", &[]);
    nv.run(&["model", "embed-pending"], "");
    nv.add("Staging postgres", "Bastion host.", &[]);
    let loads_before = nv.loader.loads;

    let info = nv.run(&["model", "info"], "");

    let folder = nv.home.path().join("models").join("bge-small-en-v1.5");
    assert_eq!(
        info.out,
        format!(
            "model: fake-bag-of-words (64 dimensions)\n\
             folder: {} (found)\n\
             notes: 1 embedded, 1 pending\n",
            folder.display()
        )
    );
    assert_eq!(nv.loader.loads, loads_before);

    let json: serde_json::Value =
        serde_json::from_str(&nv.run(&["model", "info", "--json"], "").out).unwrap();
    assert_eq!(json["model"], "fake-bag-of-words");
    assert_eq!(json["dims"], 64);
    assert_eq!(json["installed"], true);
    assert_eq!(json["embedded"], 1);
    assert_eq!(json["pending"], 1);
}

#[test]
fn search_embeds_what_the_background_missed() {
    let mut nv = Nv::new();
    nv.add("Retry 5 times", "For billing calls.", &[]);

    // The recorded background never embeds, like a background process that died.
    let found = nv.run(&["search", "how often do we retry"], "");

    assert!(found.out.starts_with("#1  "));
    assert_eq!(found.err, "");
    let info = nv.run(&["model", "info"], "");
    assert!(info.out.ends_with("notes: 1 embedded, 0 pending\n"));
}

#[test]
fn search_text_output_with_filters_matches_golden_text() {
    let mut nv = Nv::new();
    nv.add(
        "Retry 5 times",
        "For billing calls.",
        &["--type", "decision", "--repo", "billing-api"],
    );
    nv.add(
        "Retry budget",
        "At most 10% of calls.",
        &["--type", "fact", "--repo", "billing-api"],
    );
    nv.add(
        "Anna is on vacation",
        "Retry asking her on Monday.",
        &["--expires-on", "2026-10-06"],
    );
    nv.add(
        "How to retry a job",
        "Press the retry button.",
        &["--type", "how-to", "--repo", "ci"],
    );

    let found = nv.run(
        &[
            "search",
            "retry",
            "--repo",
            "billing-api",
            "--type",
            "decision",
        ],
        "",
    );
    assert_eq!(
        found.out,
        "\
#1  decision · work · 2026-10-05 · active
    Retry 5 times
    repos: billing-api
"
    );

    // The vacation note expired on 2026-10-06; today is 2026-10-07.
    let visible = nv.run(&["search", "retry", "--limit", "10"], "");
    assert!(!visible.out.contains("#3  "));
    let all = nv.run(&["search", "vacation", "--all", "--limit", "1"], "");
    assert_eq!(
        all.out,
        "\
#3  note · work · 2026-10-05 · active
    Anna is on vacation
    expires: 2026-10-06
"
    );
}

#[test]
fn only_one_embed_pending_runs_at_a_time() {
    let mut nv = Nv::new();
    nv.add("Retry 5 times", "For billing calls.", &[]);
    // Another nv process is embedding: it holds the lock.
    let lock = std::fs::File::create(nv.home.path().join("embed.lock")).unwrap();
    lock.lock().unwrap();

    let answer = nv.run(&["model", "embed-pending"], "");

    assert_eq!(answer.out, "Embedding is already running\n");
    assert_eq!(nv.loader.loads, 0);

    drop(lock);
    let answer = nv.run(&["model", "embed-pending"], "");
    assert_eq!(answer.out, "Embedded 1 notes\n");
}

#[test]
fn today_does_not_load_model() {
    let mut nv = Nv::new();
    nv.add(
        "Send retry numbers",
        "Promised.",
        &["--type", "commitment", "--planned-for", "2026-10-07"],
    );

    let today = nv.run(&["today"], "");

    assert_eq!(
        today.out,
        "Planned for today (2026-10-07)\n#1  Send retry numbers\n"
    );
    assert_eq!(nv.loader.loads, 0);
}

#[test]
fn commitment_commands_do_not_start_background_embedding() {
    let mut nv = Nv::new();
    nv.add("Send retry numbers", "Promised.", &["--type", "commitment"]);
    nv.add("Book the exam slot", "Promised.", &["--type", "commitment"]);
    let started = nv.background.started;

    // The text of the note does not change, so there is nothing to embed.
    nv.run(&["commitment", "postpone", "1", "2026-10-09"], "");
    nv.run(&["commitment", "done", "1"], "");
    nv.run(&["commitment", "drop", "2"], "");
    nv.run(&["today"], "");

    assert_eq!(nv.background.started, started);
    assert_eq!(nv.loader.loads, 0);
}

#[test]
fn replace_and_undo_start_background_embedding() {
    let mut nv = Nv::new();
    nv.add("Retry 3 times", "For billing calls.", &[]);
    assert_eq!(nv.background.started, 1);

    let replaced = nv.run(
        &[
            "note",
            "replace",
            "1",
            "--title",
            "Retry 5 times",
            "--area",
            "work",
        ],
        "Now 5.",
    );
    assert_eq!(replaced.out, "Saved #2, replaces #1\n");
    assert_eq!(nv.background.started, 2);

    // Undoing the replace changes no text of a note that stays.
    let undone = nv.run(&["history", "undo"], "");
    assert_eq!(undone.out, "Undone #3: replace of note #1\n");
    assert_eq!(nv.background.started, 2);

    nv.run(&["note", "edit", "1", "--title", "Retry three times"], "");
    assert_eq!(nv.background.started, 3);
    // Undoing an edit of the title brings old text back: it must be embedded again.
    nv.run(&["history", "undo"], "");
    assert_eq!(nv.background.started, 4);
}

#[test]
fn history_does_not_load_model() {
    let mut nv = Nv::new();
    nv.add("Retry 3 times", "For billing calls.", &[]);
    nv.add("Retry budget", "At most 10%.", &[]);

    nv.run(&["note", "link", "1", "2"], "");
    nv.run(&["history"], "");
    nv.run(&["history", "undo"], "");

    assert_eq!(nv.loader.loads, 0);
}

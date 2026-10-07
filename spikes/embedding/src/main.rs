//! Throwaway spike: bge-small-en-v1.5 via fastembed-rs, loaded from local files only.
//!
//!   embedding-spike                 run the full 25 notes / 10 queries experiment
//!   embedding-spike --cold "text"   load the model, embed one query, exit (CLI-like cold start)
//!
//! Paths can be overridden with NV_MODEL_DIR and NV_NOTES.

use std::{fs, path::PathBuf, time::Instant};

use anyhow::{Context, Result, ensure};
use fastembed::{
    InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles, UserDefinedEmbeddingModel,
};
use serde::Deserialize;

const DIMS: usize = 384;
const QUERY_PREFIX: &str = "Represent this sentence for searching relevant passages: ";
const TOP_K: usize = 3;

#[derive(Deserialize)]
struct Data {
    notes: Vec<Note>,
    queries: Vec<Query>,
}

#[derive(Deserialize)]
struct Note {
    id: i64,
    title: String,
    body: String,
}

#[derive(Deserialize)]
struct Query {
    query: String,
    expected: Vec<i64>,
}

fn model_dir() -> PathBuf {
    std::env::var_os("NV_MODEL_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../models/bge-small-en-v1.5"
            ))
        })
}

fn notes_path() -> PathBuf {
    std::env::var_os("NV_NOTES")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/notes.json")))
}

/// Load the model from bytes on disk. No hub, no cache, no network.
fn load_model() -> Result<TextEmbedding> {
    let dir = model_dir();
    let read = |name: &str| {
        fs::read(dir.join(name)).with_context(|| format!("reading {}", dir.join(name).display()))
    };
    let tokenizer_files = TokenizerFiles {
        tokenizer_file: read("tokenizer.json")?,
        config_file: read("config.json")?,
        special_tokens_map_file: read("special_tokens_map.json")?,
        tokenizer_config_file: read("tokenizer_config.json")?,
    };
    // The user-defined API has no default pooling; BGE models use the CLS token.
    let model = UserDefinedEmbeddingModel::new(read("model.onnx")?, tokenizer_files)
        .with_pooling(Pooling::Cls);
    let options = InitOptionsUserDefined::new().with_max_length(512);
    Ok(TextEmbedding::try_new_from_user_defined(model, options)?)
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Brute-force cosine (dot product on normalized vectors). Returns (note index, score).
fn top_k(query: &[f32], vectors: &[Vec<f32>], k: usize) -> Vec<(usize, f32)> {
    let mut scored: Vec<(usize, f32)> = vectors
        .iter()
        .enumerate()
        .map(|(i, v)| (i, dot(query, v)))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(k);
    scored
}

/// Peak resident memory of this process in MB (Linux only).
fn peak_rss_mb() -> Option<f64> {
    let status = fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn cold(query: &str) -> Result<()> {
    let t = Instant::now();
    let mut model = load_model()?;
    let load_ms = ms(t);
    let t = Instant::now();
    let v = model.embed([query], None)?;
    println!(
        "cold: load {load_ms:.0} ms, embed 1 query {:.1} ms, dims {}, peak RSS {:.0} MB",
        ms(t),
        v[0].len(),
        peak_rss_mb().unwrap_or(f64::NAN)
    );
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--cold") {
        return cold(args.get(2).context("--cold needs a query")?);
    }

    let data: Data = serde_json::from_str(&fs::read_to_string(notes_path())?)?;

    let t = Instant::now();
    let mut model = load_model()?;
    let load_ms = ms(t);

    // Notes: "{title}\n{body}", never a prefix.
    let texts: Vec<String> = data
        .notes
        .iter()
        .map(|n| format!("{}\n{}", n.title, n.body))
        .collect();
    let t = Instant::now();
    let vectors = model.embed(&texts, None)?;
    let embed_notes_ms = ms(t);

    ensure!(vectors.len() == data.notes.len(), "one vector per note");
    let (mut min_norm, mut max_norm) = (f32::MAX, f32::MIN);
    for v in &vectors {
        ensure!(v.len() == DIMS, "expected {DIMS} dims, got {}", v.len());
        let norm = dot(v, v).sqrt();
        ensure!((norm - 1.0).abs() < 1e-3, "vector not normalized: {norm}");
        min_norm = min_norm.min(norm);
        max_norm = max_norm.max(norm);
    }
    println!(
        "{} notes embedded, {DIMS} dims, norms {min_norm:.6}..{max_norm:.6}\n",
        vectors.len()
    );

    let variants = [("A", ""), ("B", QUERY_PREFIX)];
    let mut hits = [0usize; 2];
    let mut query_ms = [0f64; 2];
    let mut rows = Vec::new();

    for q in &data.queries {
        println!("Q: {}   expected {:?}", q.query, q.expected);
        let mut cells = Vec::new();
        for (vi, (name, prefix)) in variants.iter().enumerate() {
            let t = Instant::now();
            let qv = model.embed([format!("{prefix}{}", q.query)], None)?;
            let top = top_k(&qv[0], &vectors, TOP_K);
            query_ms[vi] += ms(t);

            let hit = top
                .iter()
                .any(|(i, _)| q.expected.contains(&data.notes[*i].id));
            hits[vi] += hit as usize;
            for (i, score) in &top {
                let n = &data.notes[*i];
                println!("  {name}  #{:<3} {score:.3}  {}", n.id, n.title);
            }
            println!("  {name}  -> {}", if hit { "HIT" } else { "MISS" });
            let ids: Vec<String> = top
                .iter()
                .map(|(i, s)| format!("#{} ({s:.2})", data.notes[*i].id))
                .collect();
            cells.push((ids.join(", "), hit));
        }
        rows.push(format!(
            "| {} | {:?} | {} | {} | {} | {} |",
            q.query,
            q.expected,
            cells[0].0,
            cells[1].0,
            if cells[0].1 { "yes" } else { "NO" },
            if cells[1].1 { "yes" } else { "NO" },
        ));
        println!();
    }

    let n = data.queries.len();
    println!("| Query | Expected | Top 3 A (no prefix) | Top 3 B (prefix) | Hit A | Hit B |");
    println!("| --- | --- | --- | --- | --- | --- |");
    rows.iter().for_each(|r| println!("{r}"));
    println!();
    println!("hit rate A (no prefix): {}/{n}", hits[0]);
    println!("hit rate B (prefix):    {}/{n}", hits[1]);
    println!("model load:             {load_ms:.0} ms");
    println!(
        "embed {} notes:         {embed_notes_ms:.0} ms ({:.1} ms/note)",
        vectors.len(),
        embed_notes_ms / vectors.len() as f64
    );
    println!(
        "avg query A (embed+search): {:.2} ms",
        query_ms[0] / n as f64
    );
    println!(
        "avg query B (embed+search): {:.2} ms",
        query_ms[1] / n as f64
    );
    if let Some(mb) = peak_rss_mb() {
        println!("peak RSS:               {mb:.0} MB");
    }
    Ok(())
}

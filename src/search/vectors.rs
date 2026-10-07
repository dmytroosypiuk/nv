//! Embeddings in SQLite: which notes are pending, storing vectors, brute-force search.

use std::collections::HashSet;

use anyhow::{Result, ensure};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};

use super::embedder::Embedder;

/// How many notes go to the model at once, and into one write transaction.
const BATCH: usize = 32;

/// The text of a note that gets embedded. Never a prefix.
pub fn embedding_text(title: &str, body: &str) -> String {
    format!("{title}\n{body}")
}

pub fn text_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Vectors are stored as little-endian `f32`, four bytes per dimension.
pub fn to_blob(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn from_blob(blob: &[u8]) -> Result<Vec<f32>> {
    ensure!(
        blob.len().is_multiple_of(4),
        "a stored vector has {} bytes, not a multiple of 4",
        blob.len()
    );
    Ok(blob
        .chunks_exact(4)
        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
        .collect())
}

/// Cosine similarity of two normalized vectors.
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// A note with no embedding for the model, or whose text changed since it was embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingNote {
    pub note_id: i64,
    pub text: String,
    pub text_hash: String,
}

pub fn pending_notes(conn: &Connection, model: &str) -> Result<Vec<PendingNote>> {
    let mut statement = conn.prepare(
        "SELECT notes.id, notes.title, notes.body, embeddings.text_hash
         FROM notes
         LEFT JOIN embeddings ON embeddings.note_id = notes.id AND embeddings.model = ?1
         ORDER BY notes.id",
    )?;
    let rows = statement.query_map([model], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut pending = Vec::new();
    for row in rows {
        let (note_id, title, body, embedded_hash) = row?;
        let text = embedding_text(&title, &body);
        let text_hash = text_hash(&text);
        if embedded_hash.as_deref() != Some(text_hash.as_str()) {
            pending.push(PendingNote {
                note_id,
                text,
                text_hash,
            });
        }
    }
    Ok(pending)
}

/// Embeds every pending note. Returns how many were embedded.
pub fn embed_pending(conn: &Connection, embedder: &mut dyn Embedder) -> Result<usize> {
    let pending = pending_notes(conn, embedder.model())?;
    for batch in pending.chunks(BATCH) {
        let texts: Vec<String> = batch.iter().map(|note| note.text.clone()).collect();
        let vectors = embedder.embed(&texts)?;
        ensure!(
            vectors.len() == batch.len(),
            "the model gave {} vectors for {} notes",
            vectors.len(),
            batch.len()
        );
        // The model ran outside the transaction, so other nv commands were not blocked.
        let tx = conn.unchecked_transaction()?;
        for (note, vector) in batch.iter().zip(&vectors) {
            // The note may have been deleted while the model was running.
            tx.execute(
                "INSERT OR REPLACE INTO embeddings (note_id, model, dims, vector, text_hash)
                 SELECT ?1, ?2, ?3, ?4, ?5 WHERE EXISTS (SELECT 1 FROM notes WHERE id = ?1)",
                params![
                    note.note_id,
                    embedder.model(),
                    vector.len() as i64,
                    to_blob(vector),
                    note.text_hash
                ],
            )?;
        }
        tx.commit()?;
    }
    Ok(pending.len())
}

/// How many notes have an embedding for the model.
pub fn embedded_count(conn: &Connection, model: &str) -> Result<usize> {
    let count: i64 = conn.query_row(
        "SELECT count(*) FROM embeddings WHERE model = ?1",
        [model],
        |row| row.get(0),
    )?;
    Ok(count as usize)
}

/// Forgets every embedding of the model, so that all notes are pending again.
pub fn clear(conn: &Connection, model: &str) -> Result<()> {
    conn.execute("DELETE FROM embeddings WHERE model = ?1", [model])?;
    Ok(())
}

/// Note IDs with their cosine similarity to `query`, most similar first.
/// With `candidates`, only those notes are looked at.
pub fn vector_search(
    conn: &Connection,
    model: &str,
    query: &[f32],
    candidates: Option<&HashSet<i64>>,
    limit: usize,
) -> Result<Vec<(i64, f32)>> {
    let mut statement = conn.prepare("SELECT note_id, vector FROM embeddings WHERE model = ?1")?;
    let rows = statement.query_map([model], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?))
    })?;
    let mut scored = Vec::new();
    for row in rows {
        let (note_id, blob) = row?;
        if candidates.is_none_or(|candidates| candidates.contains(&note_id)) {
            scored.push((note_id, dot(query, &from_blob(&blob)?)));
        }
    }
    scored.sort_by(|a, b| b.1.total_cmp(&a.1).then(b.0.cmp(&a.0)));
    scored.truncate(limit);
    Ok(scored)
}

#[cfg(test)]
#[path = "vectors_tests.rs"]
mod tests;

//! Embeddings, model, FTS and ranking. Depends on `knowledge`, never the reverse.

pub mod bge;
pub mod embedder;
pub mod filter;
pub mod fusion;
pub mod keyword;
pub mod vectors;

use std::collections::HashSet;

use anyhow::{Context, Result};
use rusqlite::Connection;

use crate::clock::Date;
use embedder::ModelLoader;
use filter::NoteFilter;

/// How many notes each ranker hands to the fusion.
pub const RANKER_DEPTH: usize = 50;

/// What to look for: free text, filters, or both.
#[derive(Debug, Clone)]
pub struct SearchRequest<'a> {
    pub text: Option<&'a str>,
    pub filter: NoteFilter,
    pub limit: usize,
    /// Injected: decides which notes are expired.
    pub today: Date,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchOutcome {
    /// Best first.
    pub note_ids: Vec<i64>,
    /// How many notes a filter-only search found before `limit` cut the list. A text
    /// search ranks every note and has no cut-off, so it has no total.
    pub total: Option<usize>,
    /// Vectors were needed but the model is not installed: keyword search only.
    pub model_missing: bool,
}

/// Hybrid search: keyword and vector rankings fused with RRF, then the database rules.
/// The model is loaded only when there is text to embed.
pub fn search(
    conn: &Connection,
    request: &SearchRequest<'_>,
    loader: &mut dyn ModelLoader,
) -> Result<SearchOutcome> {
    // Filters and expiry choose the candidates first, so they can never empty the top N.
    let candidates = request.filter.candidates(conn, request.today)?;
    let text = request.text.map(str::trim).filter(|text| !text.is_empty());

    let total = text.is_none().then_some(candidates.len());
    let mut model_missing = false;
    let mut ranked: Vec<i64> = match text {
        // Filter-only search: newest first, and no model.
        None => candidates
            .iter()
            .map(|candidate| candidate.note_id)
            .collect(),
        Some(text) => {
            let allowed: HashSet<i64> = candidates
                .iter()
                .map(|candidate| candidate.note_id)
                .collect();
            let by_keyword = keyword::keyword_search(conn, text, Some(&allowed), RANKER_DEPTH)?;
            let by_vector: Vec<i64> = match loader.load()? {
                Some(embedder) => {
                    // The background embedder may have died: nothing is missed.
                    vectors::embed_pending(conn, embedder)?;
                    let query = embedder
                        .embed(&[text.to_string()])?
                        .pop()
                        .context("the model gave no vector for the query")?;
                    vectors::vector_search(
                        conn,
                        embedder.model(),
                        &query,
                        Some(&allowed),
                        RANKER_DEPTH,
                    )?
                    .into_iter()
                    .map(|(note_id, _)| note_id)
                    .collect()
                }
                None => {
                    model_missing = true;
                    Vec::new()
                }
            };
            fusion::rrf(&[&by_keyword, &by_vector], fusion::RRF_K)
        }
    };

    // Database rule after fusion: active notes rank above outdated ones (stable sort).
    let outdated: HashSet<i64> = candidates
        .iter()
        .filter(|candidate| candidate.outdated)
        .map(|candidate| candidate.note_id)
        .collect();
    ranked.sort_by_key(|note_id| outdated.contains(note_id));
    ranked.truncate(request.limit);

    Ok(SearchOutcome {
        note_ids: ranked,
        total,
        model_missing,
    })
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;

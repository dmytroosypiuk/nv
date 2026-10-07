//! Keyword search with FTS5.

use std::collections::HashSet;

use anyhow::Result;
use rusqlite::{Connection, params};

/// A word in the title counts this many times more than a word in the body.
const TITLE_WEIGHT: f64 = 5.0;

/// IDs of the notes that hold words of `query`, best match first.
/// With `candidates`, only those notes are looked at.
pub fn keyword_search(
    conn: &Connection,
    query: &str,
    candidates: Option<&HashSet<i64>>,
    limit: usize,
) -> Result<Vec<i64>> {
    let Some(fts_query) = fts_query(query) else {
        return Ok(Vec::new());
    };
    let mut statement = conn.prepare(
        "SELECT rowid FROM notes_fts WHERE notes_fts MATCH ?1
         ORDER BY bm25(notes_fts, ?2, 1.0), rowid DESC",
    )?;
    let ids = statement.query_map(params![fts_query, TITLE_WEIGHT], |row| row.get(0))?;
    let mut found = Vec::new();
    for id in ids {
        let id: i64 = id?;
        if candidates.is_none_or(|candidates| candidates.contains(&id)) {
            found.push(id);
            if found.len() == limit {
                break;
            }
        }
    }
    Ok(found)
}

/// Turns free text into an FTS5 query: any of its words, each quoted so that nothing
/// the user typed is read as FTS5 syntax. `None` when the text has no words.
fn fts_query(query: &str) -> Option<String> {
    let words: Vec<String> = query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| format!("\"{word}\""))
        .collect();
    (!words.is_empty()).then(|| words.join(" OR "))
}

#[cfg(test)]
#[path = "keyword_tests.rs"]
mod tests;

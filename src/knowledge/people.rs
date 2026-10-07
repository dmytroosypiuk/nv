//! People: one main name and many aliases, always addressed by ID.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};

/// The main name of a person, or `None` when there is no person with this ID.
pub fn person_name(conn: &Connection, person_id: i64) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT name FROM people WHERE id = ?1",
            [person_id],
            |row| row.get(0),
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn person_is_found_by_id() {
        let conn = db::open_in_memory().unwrap();
        conn.execute("INSERT INTO people (id, name) VALUES (7, 'Anna Nowak')", [])
            .unwrap();

        assert_eq!(
            person_name(&conn, 7).unwrap().as_deref(),
            Some("Anna Nowak")
        );
        assert_eq!(person_name(&conn, 8).unwrap(), None);
    }
}

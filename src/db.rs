//! SQLite database: open, pragmas, migrations.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension};

/// How long a command waits for a lock held by another nv process.
pub const BUSY_TIMEOUT: Duration = Duration::from_secs(5);

/// Numbered migrations: the first entry brings the schema to version 1, and so on.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_initial.sql"),
    include_str!("migrations/0002_fts_triggers.sql"),
    include_str!("migrations/0003_change_log_for_people.sql"),
    include_str!("migrations/0004_undone_at.sql"),
];

/// The schema version this binary knows.
pub fn latest_schema_version() -> i64 {
    MIGRATIONS.len() as i64
}

/// Opens the database file, creating it and its folder when missing, and migrates it.
pub fn open(path: &Path) -> Result<Connection> {
    if let Some(folder) = path.parent() {
        fs::create_dir_all(folder)
            .with_context(|| format!("cannot create folder {}", folder.display()))?;
    }
    let conn = Connection::open(path)
        .with_context(|| format!("cannot open database {}", path.display()))?;
    prepare(conn)
}

/// A migrated database that lives only in memory, for tests.
pub fn open_in_memory() -> Result<Connection> {
    prepare(Connection::open_in_memory()?)
}

/// The schema version stored in `meta`; 0 for a database with no migrations applied.
pub fn schema_version(conn: &Connection) -> Result<i64> {
    let stored: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    match stored {
        Some(version) => version
            .parse()
            .with_context(|| format!("meta.schema_version is not a number: {version:?}")),
        None => Ok(0),
    }
}

fn prepare(mut conn: Connection) -> Result<Connection> {
    // The background embedder and the CLI can use the database at the same time.
    conn.busy_timeout(BUSY_TIMEOUT)?;
    conn.pragma_update_and_check(None, "journal_mode", "WAL", |_| Ok(()))?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate(&mut conn, MIGRATIONS)?;
    Ok(conn)
}

fn migrate(conn: &mut Connection, migrations: &[&str]) -> Result<()> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
        [],
    )?;
    let current = schema_version(conn)?;
    if current > latest_schema_version() {
        bail!(
            "this database was made by a newer version of nv (schema version {current}, \
             this nv knows {}): update nv",
            latest_schema_version()
        );
    }
    for (index, migration) in migrations.iter().enumerate() {
        let version = index as i64 + 1;
        if version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(migration)
            .with_context(|| format!("migration {version} failed"))?;
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [version.to_string()],
        )?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const NOW: &str = "2026-10-07 09:00:00";

    /// Inserts a note with the given type and extra commitment columns.
    fn insert_note(
        conn: &Connection,
        note_type: Option<&str>,
        extra: &str,
    ) -> rusqlite::Result<i64> {
        let (columns, values) = match extra.split_once('=') {
            Some((column, value)) => (format!(", {column}"), format!(", {value}")),
            None => (String::new(), String::new()),
        };
        conn.execute(
            &format!(
                "INSERT INTO notes (title, body, area, type, created_at, updated_at{columns})
                 VALUES ('Retry 5 times', 'We use 5 retries.', 'work', ?1, ?2, ?2{values})"
            ),
            rusqlite::params![note_type, NOW],
        )?;
        Ok(conn.last_insert_rowid())
    }

    #[test]
    fn open_creates_missing_nv_home_folder() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("not-there-yet").join("nv.db");

        open(&path).unwrap();

        assert!(path.is_file());
    }

    #[test]
    fn fresh_database_gets_latest_schema_version() {
        let dir = TempDir::new().unwrap();

        let conn = open(&dir.path().join("nv.db")).unwrap();

        assert_eq!(schema_version(&conn).unwrap(), latest_schema_version());
        assert!(latest_schema_version() >= 1);
    }

    #[test]
    fn migrations_are_idempotent() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nv.db");
        let conn = open(&path).unwrap();
        insert_note(&conn, None, "").unwrap();
        drop(conn);

        let conn = open(&path).unwrap();

        assert_eq!(schema_version(&conn).unwrap(), latest_schema_version());
        let notes: i64 = conn
            .query_row("SELECT count(*) FROM notes", [], |row| row.get(0))
            .unwrap();
        assert_eq!(notes, 1);
    }

    #[test]
    fn database_from_newer_nv_is_refused() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nv.db");
        let conn = open(&path).unwrap();
        conn.execute(
            "UPDATE meta SET value = '999' WHERE key = 'schema_version'",
            [],
        )
        .unwrap();
        drop(conn);

        let error = open(&path).unwrap_err();

        assert!(error.to_string().contains("newer version of nv"), "{error}");
    }

    #[test]
    fn database_uses_wal_and_busy_timeout() {
        let dir = TempDir::new().unwrap();

        let conn = open(&dir.path().join("nv.db")).unwrap();

        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        let busy_timeout_ms: i64 = conn
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode, "wal");
        assert_eq!(busy_timeout_ms, BUSY_TIMEOUT.as_millis() as i64);
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let conn = open_in_memory().unwrap();

        let result = conn.execute(
            "INSERT INTO note_repos (note_id, repo) VALUES (404, 'billing-api')",
            [],
        );

        assert!(result.is_err());
    }

    #[test]
    fn check_rejects_commitment_status_on_non_commitment_note() {
        let conn = open_in_memory().unwrap();

        assert!(insert_note(&conn, Some("decision"), "commitment_status='todo'").is_err());
        assert!(insert_note(&conn, None, "commitment_status='todo'").is_err());
        assert!(insert_note(&conn, Some("commitment"), "commitment_status='todo'").is_ok());
    }

    #[test]
    fn check_rejects_commitment_without_status() {
        let conn = open_in_memory().unwrap();

        assert!(insert_note(&conn, Some("commitment"), "").is_err());
    }

    #[test]
    fn check_rejects_owner_or_planned_date_on_non_commitment_note() {
        let conn = open_in_memory().unwrap();
        conn.execute("INSERT INTO people (id, name) VALUES (7, 'Anna Nowak')", [])
            .unwrap();

        assert!(insert_note(&conn, Some("fact"), "owner_person_id=7").is_err());
        assert!(insert_note(&conn, Some("fact"), "planned_for='2026-10-08'").is_err());
        assert!(insert_note(&conn, None, "closed_at='2026-10-08 10:00:00'").is_err());
        assert!(insert_note(&conn, Some("fact"), "").is_ok());
    }

    #[test]
    fn check_rejects_unknown_area() {
        let conn = open_in_memory().unwrap();

        let result = conn.execute(
            "INSERT INTO notes (title, body, area, created_at, updated_at)
             VALUES ('t', 'b', 'hobby', ?1, ?1)",
            [NOW],
        );

        assert!(result.is_err());
    }

    #[test]
    fn check_rejects_link_from_note_to_itself() {
        let conn = open_in_memory().unwrap();
        let first = insert_note(&conn, None, "").unwrap();
        let second = insert_note(&conn, None, "").unwrap();
        let link = "INSERT INTO note_links (from_id, to_id, kind) VALUES (?1, ?2, 'related')";

        assert!(conn.execute(link, [first, first]).is_err());
        assert!(conn.execute(link, [first, second]).is_ok());
    }

    #[test]
    fn fts5_is_available() {
        let conn = open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO notes_fts (rowid, title, body) VALUES (1, 'Retry 5 times', 'billing')",
            [],
        )
        .unwrap();

        let found: i64 = conn
            .query_row(
                "SELECT rowid FROM notes_fts WHERE notes_fts MATCH 'retry'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(found, 1);
    }

    #[test]
    fn migration_0004_keeps_change_log_rows_and_adds_undone_at() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &MIGRATIONS[..3]).unwrap();
        conn.execute(
            "INSERT INTO change_log (id, at, actor, action, note_id) VALUES (5, ?1, 'user', 'add', 42)",
            [NOW],
        )
        .unwrap();

        migrate(&mut conn, MIGRATIONS).unwrap();

        let (action, undone_at): (String, Option<String>) = conn
            .query_row(
                "SELECT action, undone_at FROM change_log WHERE id = 5",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(action, "add");
        assert_eq!(undone_at, None);
    }

    #[test]
    fn change_log_accepts_a_change_about_a_person() {
        let conn = open_in_memory().unwrap();
        let insert = "INSERT INTO change_log (at, actor, action, note_id, person_id)
                      VALUES (?1, 'user', 'merge', ?2, ?3)";

        assert!(
            conn.execute(insert, rusqlite::params![NOW, None::<i64>, 7])
                .is_ok()
        );
        assert!(
            conn.execute(insert, rusqlite::params![NOW, 1, None::<i64>])
                .is_ok()
        );
        // A change is always about something.
        assert!(
            conn.execute(insert, rusqlite::params![NOW, None::<i64>, None::<i64>])
                .is_err()
        );
    }

    #[test]
    fn migration_0003_keeps_existing_change_log_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn, &MIGRATIONS[..2]).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), 2);
        conn.execute(
            "INSERT INTO change_log (id, at, actor, action, note_id, before_json)
             VALUES (5, ?1, 'claude', 'edit', 42, '{\"id\":42}')",
            [NOW],
        )
        .unwrap();

        migrate(&mut conn, MIGRATIONS).unwrap();

        assert_eq!(schema_version(&conn).unwrap(), latest_schema_version());
        let row: (i64, String, String, String, i64, Option<i64>, String) = conn
            .query_row(
                "SELECT id, at, actor, action, note_id, person_id, before_json FROM change_log",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                5,
                NOW.to_string(),
                "claude".to_string(),
                "edit".to_string(),
                42,
                None,
                "{\"id\":42}".to_string()
            )
        );
    }
}

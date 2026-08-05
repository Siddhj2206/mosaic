//! SQLite access: `Db` opens/migrates a database file and hands out a
//! read-only connection for the UI thread and a write connection for the
//! background sync task (ADR-0002, per MOSAIC.md §7).

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::{Error, Result};

pub mod crud;
pub mod schema;

use crate::db::schema::MIGRATIONS;
use crate::db::crud::Conn;

/// Handle to a mosaic database file. Cheap to clone; connections are created
/// per use site (UI reader on the main thread, writer per sync run).
#[derive(Debug, Clone)]
pub struct Db {
    path: PathBuf,
}

impl Db {
    /// Open (creating if needed) and migrate the database at `path`.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                    path: parent.to_path_buf(),
                    source,
                })?;
            }
        }
        let conn = Connection::open(&path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::migrate(&conn)?;
        drop(conn);
        Ok(Db { path })
    }

    /// Open the in-memory database (tests only; migrations still applied).
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        Self::migrate(&conn)?;
        drop(conn);
        Ok(Db { path: PathBuf::from(":memory:") })
    }

    fn migrate(conn: &Connection) -> Result<()> {
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version as usize > MIGRATIONS.len() {
            return Err(Error::Migration(format!(
                "database schema version {version} is newer than this binary supports ({})",
                MIGRATIONS.len()
            )));
        }
        if (version as usize) < MIGRATIONS.len() {
            conn.execute_batch("BEGIN IMMEDIATE")?;
            for (i, migration) in MIGRATIONS.iter().enumerate().skip(version as usize) {
                conn.execute_batch(migration).map_err(|e| {
                    Error::Migration(format!("migration {} failed: {e}", i + 1))
                })?;
            }
            conn.pragma_update(None, "user_version", MIGRATIONS.len() as i64)?;
            conn.execute_batch("COMMIT")?;
        }

        // Schema guard: a database at our version must actually match our
        // schema. This catches DBs created by unrelated code that happens to
        // share a user_version number (e.g. the pre-rewrite mosaic).
        let has_normalized = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('ipos') WHERE name = 'normalized_name'",
            [],
            |row| row.get::<_, i64>(0),
        )?;
        if has_normalized == 0 {
            return Err(Error::Migration(
                "database schema does not match this binary (missing ipos.normalized_name). \
                 If this is a pre-rewrite mosaic database, move the file aside and re-run."
                    .to_string(),
            ));
        }
        Ok(())
    }

    /// The database file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read-only connection for UI queries (`PRAGMA query_only=1`).
    pub fn reader(&self) -> Result<Conn> {
        let conn = Connection::open(&self.path)?;
        conn.pragma_update(None, "query_only", true)?;
        Ok(Conn(conn))
    }

    /// Write connection for background ingestion (`busy_timeout=5000`,
    /// `synchronous=NORMAL` — safe with WAL).
    pub fn writer(&self) -> Result<Conn> {
        let conn = Connection::open(&self.path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(Conn(conn))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_and_are_idempotent() {
        let dir = std::env::temp_dir().join(format!("mosaic-db-mig-test-{}", std::process::id()));
        let path = dir.join("test.db");
        let _ = std::fs::remove_dir_all(&dir);

        let db1 = Db::open(&path).unwrap();
        let db2 = Db::open(&path).unwrap();
        drop(db1);
        drop(db2);

        let conn = Db::open(&path).unwrap().reader().unwrap();
        let v: i64 = conn
            .0
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(v, MIGRATIONS.len() as i64);
        let n: i64 = conn
            .0
            .query_row("SELECT COUNT(*) FROM markets", [], |row| row.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reader_is_query_only() {
        let db = Db::open_in_memory().unwrap();
        let conn = db.reader().unwrap();
        let err = conn.0.execute("INSERT INTO markets VALUES ('x','X','X','X')", []);
        assert!(err.is_err(), "reader connection must reject writes");
    }
}

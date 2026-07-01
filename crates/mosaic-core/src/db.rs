use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rusqlite::Connection;

pub struct MosaicDb {
    path: PathBuf,
    reader: Connection,
    writer: Mutex<Connection>,
}

impl MosaicDb {
    const MIGRATIONS: &[&str] = &[
        // Migration 1: initial schema
        "CREATE TABLE IF NOT EXISTS markets (
            id              TEXT PRIMARY KEY,
            name            TEXT NOT NULL,
            currency        TEXT NOT NULL,
            currency_symbol TEXT NOT NULL
        );

        INSERT OR IGNORE INTO markets (id, name, currency, currency_symbol) VALUES
            ('in', 'India', 'INR', '\u{20b9}'),
            ('us', 'United States', 'USD', '$'),
            ('hk', 'Hong Kong', 'HKD', 'HK$');

        CREATE TABLE IF NOT EXISTS ipos (
            id                  INTEGER PRIMARY KEY,
            market_id           TEXT NOT NULL DEFAULT 'in' REFERENCES markets(id),
            company_name        TEXT NOT NULL,
            symbol              TEXT,
            exchange            TEXT,
            sector              TEXT,
            offer_type          TEXT,
            price_band_low      REAL,
            price_band_high     REAL,
            final_price         REAL,
            lot_size            INTEGER,
            shares_offered       INTEGER,
            fresh_issue_shares   INTEGER,
            ofs_shares           INTEGER,
            shares_outstanding_post INTEGER,
            issue_size          REAL,
            open_date           TEXT,
            close_date          TEXT,
            allotment_date      TEXT,
            listing_date        TEXT,
            status              TEXT NOT NULL,
            drhp_url            TEXT,
            rhp_url             TEXT,
            source              TEXT NOT NULL,
            ingested_at         TEXT NOT NULL,
            updated_at          TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS subscription_snapshots (
            id          INTEGER PRIMARY KEY,
            ipo_id      INTEGER NOT NULL REFERENCES ipos(id),
            snapshot_at TEXT NOT NULL,
            category    TEXT NOT NULL,
            subscribed  REAL,
            source      TEXT NOT NULL,
            ingested_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS price_history (
            id          INTEGER PRIMARY KEY,
            ipo_id      INTEGER NOT NULL REFERENCES ipos(id),
            trade_date  TEXT NOT NULL,
            open_price  REAL,
            high_price  REAL,
            low_price   REAL,
            close_price REAL,
            volume      INTEGER,
            source      TEXT NOT NULL,
            ingested_at TEXT NOT NULL,
            UNIQUE(ipo_id, trade_date)
        );

        CREATE TABLE IF NOT EXISTS ingestion_runs (
            id              INTEGER PRIMARY KEY,
            source          TEXT NOT NULL,
            started_at      TEXT NOT NULL,
            finished_at     TEXT,
            status          TEXT,
            records_written INTEGER,
            notes           TEXT
        );

        CREATE TABLE IF NOT EXISTS exchange_rates (
            id            INTEGER PRIMARY KEY,
            from_currency TEXT NOT NULL,
            to_currency   TEXT NOT NULL,
            date          TEXT NOT NULL,
            rate          REAL NOT NULL,
            source        TEXT NOT NULL,
            UNIQUE(from_currency, to_currency, date)
        );

        CREATE TABLE IF NOT EXISTS key_value_store (
            key   TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_sub_ipo_cat ON subscription_snapshots(ipo_id, snapshot_at, category);
        CREATE INDEX IF NOT EXISTS idx_price_ipo ON price_history(ipo_id, trade_date);
        CREATE INDEX IF NOT EXISTS idx_ipos_status ON ipos(status);
        CREATE INDEX IF NOT EXISTS idx_ipos_market ON ipos(market_id);
        CREATE INDEX IF NOT EXISTS idx_xrate_pair ON exchange_rates(from_currency, to_currency, date);",
    ];

    pub fn open(path: impl AsRef<Path>) -> Result<Self, rusqlite::Error> {
        let path = path.as_ref().to_path_buf();

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }

        let writer = Connection::open(&path)?;
        writer.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;")?;

        let reader = Connection::open(&path)?;
        reader.execute_batch("PRAGMA journal_mode=WAL; PRAGMA query_only=1;")?;

        let mut db = MosaicDb {
            path,
            reader,
            writer: Mutex::new(writer),
        };
        db.run_migrations()?;
        Ok(db)
    }

    pub fn open_reader(path: impl AsRef<Path>) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA query_only=1;")?;
        Ok(conn)
    }

    pub fn open_writer(path: impl AsRef<Path>) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000; PRAGMA synchronous=NORMAL;")?;
        Ok(conn)
    }

    fn run_migrations(&mut self) -> Result<(), rusqlite::Error> {
        let current_version: i32 = self
            .writer
            .lock()
            .unwrap()
            .pragma_query_value(None, "user_version", |row| row.get::<_, i32>(0))?;

        let writer = self.writer.lock().unwrap();
        for (i, migration) in Self::MIGRATIONS.iter().enumerate().skip(current_version as usize) {
            writer.execute_batch(migration)?;
            let new_version = (i + 1) as i32;
            writer.pragma_update(None, "user_version", new_version)?;
        }

        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn reader(&self) -> &Connection {
        &self.reader
    }

    pub fn writer(&self) -> &Mutex<Connection> {
        &self.writer
    }
}

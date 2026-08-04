//! SQLite schema: migrations via a static ordered array + `PRAGMA user_version`.
//!
//! Migration 1 is the v1 schema per ADR-0002: `markets` (seeded India),
//! `ipos` (mutable current-state row), `subscription_snapshots` and
//! `price_history` (append-only with UNIQUE upsert keys), `ingestion_runs`
//! (audit), and `key_value_store` (window/panel state).

pub const MIGRATIONS: &[&str] = &[r#"
CREATE TABLE markets (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    currency        TEXT NOT NULL,
    currency_symbol TEXT NOT NULL
);
INSERT INTO markets (id, name, currency, currency_symbol) VALUES
    ('in', 'India', 'INR', '₹');

CREATE TABLE ipos (
    id                    INTEGER PRIMARY KEY,
    company_name          TEXT NOT NULL,
    normalized_name       TEXT NOT NULL,
    symbol                TEXT,
    exchange              TEXT,
    sector                TEXT,
    status                TEXT NOT NULL,
    price_band_low        REAL,
    price_band_high       REAL,
    final_price           REAL,
    face_value            REAL,
    lot_size              INTEGER,
    lot_multiples         INTEGER,
    issue_size_cr         REAL,
    shares_offered        INTEGER,
    fresh_issue_shares    INTEGER,
    ofs_shares            INTEGER,
    issue_type            TEXT,
    offer_type            TEXT,
    open_date             TEXT,
    close_date            TEXT,
    allotment_date        TEXT,
    listing_date          TEXT,
    listing_date_tentative INTEGER NOT NULL DEFAULT 0,
    drhp_url              TEXT,
    rhp_url               TEXT,
    source                TEXT NOT NULL,
    ingested_at           TEXT NOT NULL,
    updated_at            TEXT NOT NULL
);
CREATE UNIQUE INDEX idx_ipos_symbol ON ipos(symbol) WHERE symbol IS NOT NULL;
CREATE UNIQUE INDEX idx_ipos_normalized ON ipos(normalized_name);
CREATE INDEX idx_ipos_status ON ipos(status);
CREATE INDEX idx_ipos_open_date ON ipos(open_date);

CREATE TABLE subscription_snapshots (
    id               INTEGER PRIMARY KEY,
    ipo_id           INTEGER NOT NULL REFERENCES ipos(id),
    snapshot_at      TEXT NOT NULL,
    category         TEXT NOT NULL,
    offered_shares   INTEGER,
    bid_shares       INTEGER,
    times_subscribed REAL,
    source           TEXT NOT NULL,
    ingested_at      TEXT NOT NULL,
    UNIQUE(ipo_id, snapshot_at, category)
);
CREATE INDEX idx_sub_ipo_snapshot ON subscription_snapshots(ipo_id, snapshot_at);

CREATE TABLE price_history (
    id          INTEGER PRIMARY KEY,
    ipo_id      INTEGER NOT NULL REFERENCES ipos(id),
    trade_date  TEXT NOT NULL,
    open_price  REAL,
    high_price  REAL,
    low_price   REAL,
    close_price REAL,
    volume      INTEGER,
    vwap        REAL,
    source      TEXT NOT NULL,
    ingested_at TEXT NOT NULL,
    UNIQUE(ipo_id, trade_date)
);
CREATE INDEX idx_price_ipo ON price_history(ipo_id, trade_date);

CREATE TABLE ingestion_runs (
    id              INTEGER PRIMARY KEY,
    source          TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    status          TEXT,
    records_written INTEGER NOT NULL DEFAULT 0,
    notes           TEXT
);

CREATE TABLE key_value_store (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#];

//! CRUD operations on a `Conn` wrapper around `rusqlite::Connection`.
//!
//! (Rust 2024 forbids inherent impls on foreign types, so read/write methods
//! live on `Conn`, which `Db::reader()` / `Db::writer()` hand out.)

use jiff::civil::{Date, DateTime};
use rusqlite::{Connection, OptionalExtension, Row, params};
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};

use crate::error::{Error, Result};
use crate::types::{
    IngestionRun, Ipo, IpoStatus, PricePoint, RunStatus, SubscriptionSnapshot,
};

/// A connection to the mosaic database. Reads take `&self`; writes take
/// `&mut self` (rusqlite's `Connection` requires `&mut` for writes anyway).
pub struct Conn(pub Connection);

// ---------------------------------------------------------------------------
// Row mapping helpers
// ---------------------------------------------------------------------------

fn opt_date(s: Option<String>) -> Option<Date> {
    s.and_then(|s| s.parse().ok())
}

fn opt_dt(s: Option<String>) -> Option<DateTime> {
    s.and_then(|s| s.parse().ok())
}

fn opt_dec(f: Option<f64>) -> Option<Decimal> {
    f.and_then(Decimal::from_f64)
}

fn parse_date_col(row: &Row, idx: usize) -> rusqlite::Result<Date> {
    let s: String = row.get(idx)?;
    s.parse().map_err(|e: jiff::Error| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, e.into())
    })
}

fn parse_dt_col(row: &Row, idx: usize) -> rusqlite::Result<DateTime> {
    let s: String = row.get(idx)?;
    s.parse().map_err(|e: jiff::Error| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, e.into())
    })
}

fn map_ipo(row: &Row) -> rusqlite::Result<Ipo> {
    let status: String = row.get(6)?;
    Ok(Ipo {
        id: Some(row.get(0)?),
        company_name: row.get(1)?,
        normalized_name: row.get(2)?,
        symbol: row.get(3)?,
        exchange: row.get(4)?,
        sector: row.get(5)?,
        status: status.parse().map_err(|e: String| {
            rusqlite::Error::FromSqlConversionFailure(6, rusqlite::types::Type::Text, e.into())
        })?,
        price_band_low: opt_dec(row.get(7)?),
        price_band_high: opt_dec(row.get(8)?),
        final_price: opt_dec(row.get(9)?),
        face_value: opt_dec(row.get(10)?),
        lot_size: row.get(11)?,
        lot_multiples: row.get(12)?,
        issue_size_cr: opt_dec(row.get(13)?),
        shares_offered: row.get(14)?,
        fresh_issue_shares: row.get(15)?,
        ofs_shares: row.get(16)?,
        issue_type: row.get(17)?,
        offer_type: row.get(18)?,
        open_date: opt_date(row.get(19)?),
        close_date: opt_date(row.get(20)?),
        allotment_date: opt_date(row.get(21)?),
        listing_date: opt_date(row.get(22)?),
        listing_date_tentative: row.get::<_, i64>(23)? != 0,
        drhp_url: row.get(24)?,
        rhp_url: row.get(25)?,
        source: row.get(26)?,
        ingested_at: parse_dt_col(row, 27)?,
        updated_at: parse_dt_col(row, 28)?,
    })
}

const IPO_COLS: &str = "id, company_name, normalized_name, symbol, exchange, sector, status, \
    price_band_low, price_band_high, final_price, face_value, lot_size, lot_multiples, \
    issue_size_cr, shares_offered, fresh_issue_shares, ofs_shares, issue_type, offer_type, \
    open_date, close_date, allotment_date, listing_date, listing_date_tentative, drhp_url, \
    rhp_url, source, ingested_at, updated_at";

/// Per-status counts for the list stat strip.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct StatusCounts {
    pub upcoming: i64,
    pub open: i64,
    pub closed: i64,
    pub listed: i64,
}

// ---------------------------------------------------------------------------
// Reads
// ---------------------------------------------------------------------------

impl Conn {
    /// List IPOs, optionally filtered by status and searched by name.
    pub fn list_ipos(&self, status: Option<IpoStatus>, search: Option<&str>) -> Result<Vec<Ipo>> {
        let mut sql = format!("SELECT {IPO_COLS} FROM ipos");
        let mut where_clauses: Vec<String> = Vec::new();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(s) = status {
            where_clauses.push("status = ?".to_string());
            params_vec.push(Box::new(s.as_str().to_string()));
        }
        if let Some(s) = search {
            let s = s.trim();
            if !s.is_empty() {
                where_clauses.push("(company_name LIKE ? OR normalized_name LIKE ?)".to_string());
                let like = format!("%{}%", s.to_ascii_lowercase());
                params_vec.push(Box::new(like.clone()));
                params_vec.push(Box::new(like));
            }
        }
        if !where_clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&where_clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY open_date IS NULL, open_date ASC, id ASC");

        let mut stmt = self.0.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params.as_slice(), map_ipo)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Look up an IPO by exact symbol (NSE ticker).
    pub fn ipo_by_symbol(&self, symbol: &str) -> Result<Option<Ipo>> {
        self.0
            .query_row(
                &format!("SELECT {IPO_COLS} FROM ipos WHERE symbol = ?1"),
                [symbol],
                map_ipo,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Look up an IPO by normalized company name.
    pub fn ipo_by_normalized_name(&self, normalized: &str) -> Result<Option<Ipo>> {
        self.0
            .query_row(
                &format!("SELECT {IPO_COLS} FROM ipos WHERE normalized_name = ?1"),
                [normalized],
                map_ipo,
            )
            .optional()
            .map_err(Into::into)
    }

    /// Subscription snapshots for one IPO, ordered by day then category.
    pub fn list_subscriptions(&self, ipo_id: i64) -> Result<Vec<SubscriptionSnapshot>> {
        let mut stmt = self.0.prepare(
            "SELECT id, ipo_id, snapshot_at, category, offered_shares, bid_shares, \
             times_subscribed, source, ingested_at FROM subscription_snapshots \
             WHERE ipo_id = ?1 ORDER BY snapshot_at ASC, category ASC",
        )?;
        let rows = stmt.query_map([ipo_id], |row| {
            let category: String = row.get(3)?;
            Ok(SubscriptionSnapshot {
                id: Some(row.get(0)?),
                ipo_id: Some(row.get(1)?),
                snapshot_at: parse_date_col(row, 2)?,
                category: category.parse().map_err(|e: String| {
                    rusqlite::Error::FromSqlConversionFailure(
                        3,
                        rusqlite::types::Type::Text,
                        e.into(),
                    )
                })?,
                offered_shares: row.get(4)?,
                bid_shares: row.get(5)?,
                times_subscribed: opt_dec(row.get(6)?),
                source: row.get(7)?,
                ingested_at: parse_dt_col(row, 8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// EOD price history for one IPO, ordered by trade date.
    pub fn list_price_history(&self, ipo_id: i64) -> Result<Vec<PricePoint>> {
        let mut stmt = self.0.prepare(
            "SELECT id, ipo_id, trade_date, open_price, high_price, low_price, close_price, \
             volume, vwap, source, ingested_at FROM price_history \
             WHERE ipo_id = ?1 ORDER BY trade_date ASC",
        )?;
        let rows = stmt.query_map([ipo_id], |row| {
            Ok(PricePoint {
                id: Some(row.get(0)?),
                ipo_id: Some(row.get(1)?),
                trade_date: parse_date_col(row, 2)?,
                open_price: opt_dec(row.get(3)?),
                high_price: opt_dec(row.get(4)?),
                low_price: opt_dec(row.get(5)?),
                close_price: opt_dec(row.get(6)?),
                volume: row.get(7)?,
                vwap: opt_dec(row.get(8)?),
                source: row.get(9)?,
                ingested_at: parse_dt_col(row, 10)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn status_counts(&self) -> Result<StatusCounts> {
        let mut counts = StatusCounts::default();
        let mut stmt = self
            .0
            .prepare("SELECT status, COUNT(*) FROM ipos GROUP BY status")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?;
        for row in rows {
            let (status, n) = row?;
            match status.as_str() {
                "upcoming" => counts.upcoming = n,
                "open" => counts.open = n,
                "closed" => counts.closed = n,
                "listed" => counts.listed = n,
                _ => {}
            }
        }
        Ok(counts)
    }

    /// Latest ingestion runs (most recent first).
    pub fn list_runs(&self, limit: i64) -> Result<Vec<IngestionRun>> {
        let mut stmt = self.0.prepare(
            "SELECT id, source, started_at, finished_at, status, records_written, notes \
             FROM ingestion_runs ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map([limit], |row| {
            let status_str: Option<String> = row.get(4)?;
            let status = match status_str.as_deref() {
                Some("success") => RunStatus::Success,
                Some("partial") => RunStatus::Partial,
                _ => RunStatus::Failed,
            };
            Ok(IngestionRun {
                id: Some(row.get(0)?),
                source: row.get(1)?,
                started_at: parse_dt_col(row, 2)?,
                finished_at: opt_dt(row.get(3)?),
                status: Some(status),
                records_written: row.get(5)?,
                notes: row.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    /// Read a value from the key-value store.
    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        self.0
            .query_row(
                "SELECT value FROM key_value_store WHERE key = ?1",
                [key],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }
}

// ---------------------------------------------------------------------------
// Writes
// ---------------------------------------------------------------------------

impl Conn {
    /// Upsert IPOs. Matching is by symbol when present, else normalized name.
    /// Returns the number of rows written (inserts + updates).
    pub fn upsert_ipos(&mut self, ipos: &[Ipo]) -> Result<usize> {
        let tx = self.0.transaction()?;
        let mut written = 0;
        for ipo in ipos {
            let existing = if let Some(symbol) = &ipo.symbol {
                tx.query_row("SELECT id FROM ipos WHERE symbol = ?1", [symbol], |row| {
                    row.get::<_, i64>(0)
                })
                .optional()?
            } else {
                None
            };
            let existing = match existing {
                Some(id) => Some(id),
                None => tx
                    .query_row(
                        "SELECT id FROM ipos WHERE normalized_name = ?1",
                        [&ipo.normalized_name],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()?,
            };

            let now = crate::types::now_utc();
            let updated_at = ipo.updated_at.max(now);
            match existing {
                Some(id) => {
                    tx.execute(
                        "UPDATE ipos SET
                           company_name = ?1, normalized_name = ?2, symbol = ?3, exchange = ?4,
                           sector = ?5, status = ?6, price_band_low = ?7, price_band_high = ?8,
                           final_price = ?9, face_value = ?10, lot_size = ?11, lot_multiples = ?12,
                           issue_size_cr = ?13, shares_offered = ?14, fresh_issue_shares = ?15,
                           ofs_shares = ?16, issue_type = ?17, offer_type = ?18, open_date = ?19,
                           close_date = ?20, allotment_date = ?21, listing_date = ?22,
                           listing_date_tentative = ?23, drhp_url = ?24, rhp_url = ?25,
                           source = ?26, updated_at = ?27
                         WHERE id = ?28",
                        params![
                            ipo.company_name,
                            ipo.normalized_name,
                            ipo.symbol,
                            ipo.exchange,
                            ipo.sector,
                            ipo.status.as_str(),
                            ipo.price_band_low.and_then(|d| d.to_f64()),
                            ipo.price_band_high.and_then(|d| d.to_f64()),
                            ipo.final_price.and_then(|d| d.to_f64()),
                            ipo.face_value.and_then(|d| d.to_f64()),
                            ipo.lot_size,
                            ipo.lot_multiples,
                            ipo.issue_size_cr.and_then(|d| d.to_f64()),
                            ipo.shares_offered,
                            ipo.fresh_issue_shares,
                            ipo.ofs_shares,
                            ipo.issue_type,
                            ipo.offer_type,
                            ipo.open_date.map(|d| d.to_string()),
                            ipo.close_date.map(|d| d.to_string()),
                            ipo.allotment_date.map(|d| d.to_string()),
                            ipo.listing_date.map(|d| d.to_string()),
                            ipo.listing_date_tentative as i64,
                            ipo.drhp_url,
                            ipo.rhp_url,
                            ipo.source,
                            updated_at.to_string(),
                            id,
                        ],
                    )?;
                }
                None => {
                    tx.execute(
                        "INSERT INTO ipos (company_name, normalized_name, symbol, exchange, \
                         sector, status, price_band_low, price_band_high, final_price, \
                         face_value, lot_size, lot_multiples, issue_size_cr, shares_offered, \
                         fresh_issue_shares, ofs_shares, issue_type, offer_type, open_date, \
                         close_date, allotment_date, listing_date, listing_date_tentative, \
                         drhp_url, rhp_url, source, ingested_at, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, \
                                 ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, \
                                 ?27, ?28)",
                        params![
                            ipo.company_name,
                            ipo.normalized_name,
                            ipo.symbol,
                            ipo.exchange,
                            ipo.sector,
                            ipo.status.as_str(),
                            ipo.price_band_low.and_then(|d| d.to_f64()),
                            ipo.price_band_high.and_then(|d| d.to_f64()),
                            ipo.final_price.and_then(|d| d.to_f64()),
                            ipo.face_value.and_then(|d| d.to_f64()),
                            ipo.lot_size,
                            ipo.lot_multiples,
                            ipo.issue_size_cr.and_then(|d| d.to_f64()),
                            ipo.shares_offered,
                            ipo.fresh_issue_shares,
                            ipo.ofs_shares,
                            ipo.issue_type,
                            ipo.offer_type,
                            ipo.open_date.map(|d| d.to_string()),
                            ipo.close_date.map(|d| d.to_string()),
                            ipo.allotment_date.map(|d| d.to_string()),
                            ipo.listing_date.map(|d| d.to_string()),
                            ipo.listing_date_tentative as i64,
                            ipo.drhp_url,
                            ipo.rhp_url,
                            ipo.source,
                            ipo.ingested_at.to_string(),
                            updated_at.to_string(),
                        ],
                    )?;
                }
            }
            written += 1;
        }
        tx.commit()?;
        Ok(written)
    }

    /// Upsert subscription snapshots: UNIQUE(ipo_id, snapshot_at, category).
    /// Same-day re-poll overwrites (NSE revises intra-day).
    pub fn upsert_subscription_snapshots(
        &mut self,
        snapshots: &[SubscriptionSnapshot],
    ) -> Result<usize> {
        let tx = self.0.transaction()?;
        let mut written = 0;
        for s in snapshots {
            let ipo_id =
                s.ipo_id.ok_or_else(|| Error::Other("snapshot missing ipo_id".into()))?;
            tx.execute(
                "INSERT INTO subscription_snapshots \
                 (ipo_id, snapshot_at, category, offered_shares, bid_shares, times_subscribed, \
                  source, ingested_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
                 ON CONFLICT(ipo_id, snapshot_at, category) DO UPDATE SET \
                   offered_shares = excluded.offered_shares, \
                   bid_shares = excluded.bid_shares, \
                   times_subscribed = excluded.times_subscribed, \
                   source = excluded.source, \
                   ingested_at = excluded.ingested_at",
                params![
                    ipo_id,
                    s.snapshot_at.to_string(),
                    s.category.as_str(),
                    s.offered_shares,
                    s.bid_shares,
                    s.times_subscribed.and_then(|d| d.to_f64()),
                    s.source,
                    s.ingested_at.to_string(),
                ],
            )?;
            written += 1;
        }
        tx.commit()?;
        Ok(written)
    }

    /// Upsert price history: UNIQUE(ipo_id, trade_date). Re-ingestion upserts.
    pub fn upsert_price_history(&mut self, points: &[PricePoint]) -> Result<usize> {
        let tx = self.0.transaction()?;
        let mut written = 0;
        for p in points {
            let ipo_id =
                p.ipo_id.ok_or_else(|| Error::Other("price missing ipo_id".into()))?;
            tx.execute(
                "INSERT INTO price_history \
                 (ipo_id, trade_date, open_price, high_price, low_price, close_price, volume, \
                  vwap, source, ingested_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT(ipo_id, trade_date) DO UPDATE SET \
                   open_price = excluded.open_price, high_price = excluded.high_price, \
                   low_price = excluded.low_price, close_price = excluded.close_price, \
                   volume = excluded.volume, vwap = excluded.vwap, \
                   source = excluded.source, ingested_at = excluded.ingested_at",
                params![
                    ipo_id,
                    p.trade_date.to_string(),
                    p.open_price.and_then(|d| d.to_f64()),
                    p.high_price.and_then(|d| d.to_f64()),
                    p.low_price.and_then(|d| d.to_f64()),
                    p.close_price.and_then(|d| d.to_f64()),
                    p.volume,
                    p.vwap.and_then(|d| d.to_f64()),
                    p.source,
                    p.ingested_at.to_string(),
                ],
            )?;
            written += 1;
        }
        tx.commit()?;
        Ok(written)
    }

    /// Write a key-value pair (window state etc.).
    pub fn kv_set(&mut self, key: &str, value: &str) -> Result<()> {
        self.0.execute(
            "INSERT INTO key_value_store (key, value) VALUES (?1, ?2) \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    /// Record the start of an ingestion run; returns the run id.
    pub fn start_run(&mut self, source: &str) -> Result<i64> {
        self.0.execute(
            "INSERT INTO ingestion_runs (source, started_at) VALUES (?1, ?2)",
            params![source, crate::types::now_utc().to_string()],
        )?;
        Ok(self.0.last_insert_rowid())
    }

    /// Finish an ingestion run.
    pub fn finish_run(
        &mut self,
        run_id: i64,
        status: RunStatus,
        records_written: i64,
        notes: Option<&str>,
    ) -> Result<()> {
        self.0.execute(
            "UPDATE ingestion_runs SET finished_at = ?1, status = ?2, records_written = ?3, \
             notes = ?4 WHERE id = ?5",
            params![
                crate::types::now_utc().to_string(),
                status.as_str(),
                records_written,
                notes,
                run_id,
            ],
        )?;
        Ok(())
    }
}

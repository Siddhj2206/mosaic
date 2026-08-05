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
use crate::Db;

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
        detail_url: row.get(26)?,
        source: row.get(27)?,
        ingested_at: parse_dt_col(row, 28)?,
        updated_at: parse_dt_col(row, 29)?,
    })
}

const IPO_COLS: &str = "id, company_name, normalized_name, symbol, exchange, sector, status, \
    price_band_low, price_band_high, final_price, face_value, lot_size, lot_multiples, \
    issue_size_cr, shares_offered, fresh_issue_shares, ofs_shares, issue_type, offer_type, \
    open_date, close_date, allotment_date, listing_date, listing_date_tentative, drhp_url, \
    rhp_url, detail_url, source, ingested_at, updated_at";

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

    /// Latest snapshot per category for one IPO (most recent poll day).
    pub fn latest_subscription_by_category(&self, ipo_id: i64) -> Result<Vec<SubscriptionSnapshot>> {
        let mut stmt = self.0.prepare(
            "SELECT id, ipo_id, snapshot_at, category, offered_shares, bid_shares, \
             times_subscribed, source, ingested_at FROM subscription_snapshots \
             WHERE ipo_id = ?1 AND snapshot_at = (SELECT MAX(snapshot_at) FROM \
             subscription_snapshots WHERE ipo_id = ?1) ORDER BY category ASC",
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
                           detail_url = ?26, source = ?27, updated_at = ?28
                         WHERE id = ?29",
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
                            ipo.detail_url,
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
                         drhp_url, rhp_url, detail_url, source, ingested_at, updated_at) \
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, \
                                 ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, \
                                 ?27, ?28, ?29)",
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
                            ipo.detail_url,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Ipo, PricePoint, SubCategory, SubscriptionSnapshot, now_utc};

    static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// Unique temp path per test (parallel tests must not share a file).
    fn temp_db() -> Db {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "mosaic-crud-test-{}-{n}",
            std::process::id()
        ));
        let path = dir.join("crud.db");
        let _ = std::fs::remove_dir_all(&dir);
        Db::open(&path).unwrap()
    }

    #[test]
    fn ipo_upsert_roundtrip_preserves_all_fields() {
        let db = temp_db();
        let mut conn = db.writer().unwrap();

        let mut ipo = Ipo::new("Ardee Industries Limited", "nse");
        ipo.symbol = Some("ARDEE".into());
        ipo.exchange = Some("NSE".into());
        ipo.status = IpoStatus::Open;
        ipo.price_band_low = Some(rust_decimal::Decimal::from(50));
        ipo.price_band_high = Some(rust_decimal::Decimal::from(53));
        ipo.lot_size = Some(281);
        ipo.open_date = Some(jiff::civil::Date::constant(2026, 8, 5));
        ipo.close_date = Some(jiff::civil::Date::constant(2026, 8, 7));
        ipo.listing_date = Some(jiff::civil::Date::constant(2026, 8, 11));
        ipo.rhp_url = Some("https://nsearchives.nseindia.com/RHP_ARDEE.zip".into());
        ipo.detail_url = Some("https://www.chittorgarh.com/ipo/ardee-industries-ipo/2860/".into());
        ipo.fresh_issue_shares = Some(58422516);

        assert_eq!(conn.upsert_ipos(&[ipo]).unwrap(), 1);

        let reader = db.reader().unwrap();
        let fetched = reader.ipo_by_symbol("ARDEE").unwrap().expect("ipo by symbol");
        assert_eq!(fetched.company_name, "Ardee Industries Limited");
        assert_eq!(fetched.normalized_name, "ardee industries");
        assert_eq!(fetched.status, IpoStatus::Open);
        assert_eq!(fetched.price_band_low, Some(rust_decimal::Decimal::from(50)));
        assert_eq!(fetched.lot_size, Some(281));
        assert_eq!(fetched.open_date, Some(jiff::civil::Date::constant(2026, 8, 5)));
        assert_eq!(fetched.listing_date, Some(jiff::civil::Date::constant(2026, 8, 11)));
        assert_eq!(fetched.rhp_url.as_deref(), Some("https://nsearchives.nseindia.com/RHP_ARDEE.zip"));
        assert_eq!(fetched.detail_url.as_deref(), Some("https://www.chittorgarh.com/ipo/ardee-industries-ipo/2860/"));
        assert_eq!(fetched.fresh_issue_shares, Some(58422516));

        // Idempotent re-upsert (same symbol) must not duplicate.
        let mut again = fetched.clone();
        again.status = IpoStatus::Closed;
        assert_eq!(conn.upsert_ipos(&[again]).unwrap(), 1);
        let all = reader.list_ipos(None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].status, IpoStatus::Closed);

        // Matching by normalized name when no symbol.
        let mut other = Ipo::new("Manipal Health Enterprises", "ipowatch");
        other.open_date = Some(jiff::civil::Date::constant(2026, 7, 29));
        assert_eq!(conn.upsert_ipos(&[other]).unwrap(), 1);
        let matched = reader
            .ipo_by_normalized_name("manipal health enterprises")
            .unwrap()
            .expect("by normalized name");
        assert_eq!(matched.source, "ipowatch");

        // Status filter + search.
        assert_eq!(reader.list_ipos(Some(IpoStatus::Closed), None).unwrap().len(), 1);
        assert_eq!(reader.list_ipos(None, Some("manipal")).unwrap().len(), 1);
        assert_eq!(reader.list_ipos(None, Some("nope")).unwrap().len(), 0);
        let counts = reader.status_counts().unwrap();
        assert_eq!(counts.closed, 1);
        assert_eq!(counts.open, 0);
    }

    #[test]
    fn snapshot_and_price_upserts_are_day_wise_idempotent() {
        let db = temp_db();
        let mut conn = db.writer().unwrap();

        let mut ipo = Ipo::new("Test IPO", "test");
        ipo.symbol = Some("TEST".into());
        conn.upsert_ipos(&[ipo]).unwrap();
        let id = conn.ipo_by_symbol("TEST").unwrap().unwrap().id.unwrap();

        let day1 = jiff::civil::Date::constant(2026, 8, 5);
        let day2 = jiff::civil::Date::constant(2026, 8, 6);

        let mk = |day: jiff::civil::Date, cat: SubCategory, times: f64| {
            let mut s = SubscriptionSnapshot::new(id, day, cat, "nse");
            s.times_subscribed = rust_decimal::Decimal::from_f64(times);
            s
        };

        // Day 1: only Total.
        conn.upsert_subscription_snapshots(&[mk(day1, SubCategory::Total, 0.5)]).unwrap();
        // Day 2: full set.
        conn.upsert_subscription_snapshots(&[
            mk(day2, SubCategory::Qib, 2.1),
            mk(day2, SubCategory::Nii, 1.4),
            mk(day2, SubCategory::Retail, 8.9),
            mk(day2, SubCategory::Total, 4.2),
        ]).unwrap();
        // Same-day revision of Total.
        conn.upsert_subscription_snapshots(&[mk(day2, SubCategory::Total, 4.8)]).unwrap();

        let reader = db.reader().unwrap();
        let rows = reader.list_subscriptions(id).unwrap();
        assert_eq!(rows.len(), 5); // day1 total + 4 day2 rows (revised, not duplicated)
        let latest = reader.latest_subscription_by_category(id).unwrap();
        assert_eq!(latest.len(), 4);
        let total = latest.iter().find(|s| s.category == SubCategory::Total).unwrap();
        assert_eq!(total.snapshot_at, day2);
        assert_eq!(total.times_subscribed, rust_decimal::Decimal::from_f64(4.8));

        // Prices: day-wise, one row per date.
        let mut p1 = PricePoint::new(id, day1, "nse");
        p1.close_price = Some(rust_decimal::Decimal::from(55));
        let mut p2 = PricePoint::new(id, day2, "nse");
        p2.close_price = Some(rust_decimal::Decimal::from(58));
        conn.upsert_price_history(&[p1, p2]).unwrap();
        let mut p2b = PricePoint::new(id, day2, "nse");
        p2b.close_price = Some(rust_decimal::Decimal::from(57));
        conn.upsert_price_history(&[p2b]).unwrap();

        let prices = reader.list_price_history(id).unwrap();
        assert_eq!(prices.len(), 2);
        assert_eq!(prices[1].close_price, Some(rust_decimal::Decimal::from(57)));
    }

    #[test]
    fn kv_store_roundtrip_and_run_log() {
        let db = temp_db();
        let mut conn = db.writer().unwrap();
        conn.kv_set("window_bounds", r#"{"x":10}"#).unwrap();
        assert_eq!(
            conn.kv_get("window_bounds").unwrap().as_deref(),
            Some(r#"{"x":10}"#)
        );

        let run_id = conn.start_run("nse").unwrap();
        conn.finish_run(run_id, RunStatus::Success, 12, None).unwrap();
        let runs = conn.list_runs(5).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].source, "nse");
        assert_eq!(runs[0].status, Some(RunStatus::Success));
        assert_eq!(runs[0].records_written, 12);
        assert!(runs[0].finished_at.is_some());
        let _ = now_utc;
    }
}

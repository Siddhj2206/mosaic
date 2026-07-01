use crate::error::DbError;
use crate::types::*;
use rusqlite::params;

use super::types_sql::{decimal_to_f64_opt, f64_to_decimal_opt};
use super::MosaicDb;

impl MosaicDb {
    pub fn insert_ipo(&self, ipo: &Ipo) -> Result<i64, DbError> {
        let conn = self.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO ipos (market_id, company_name, symbol, exchange, sector, offer_type,
             price_band_low, price_band_high, final_price, lot_size, shares_offered,
             fresh_issue_shares, ofs_shares, shares_outstanding_post, issue_size,
             open_date, close_date, allotment_date, listing_date, status,
             drhp_url, rhp_url, source, ingested_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
             ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)",
            params![
                ipo.market_id,
                ipo.company_name,
                ipo.symbol,
                ipo.exchange,
                ipo.sector,
                ipo.offer_type,
                decimal_to_f64_opt(ipo.price_band_low),
                decimal_to_f64_opt(ipo.price_band_high),
                decimal_to_f64_opt(ipo.final_price),
                ipo.lot_size,
                ipo.shares_offered,
                ipo.fresh_issue_shares,
                ipo.ofs_shares,
                ipo.shares_outstanding_post,
                decimal_to_f64_opt(ipo.issue_size),
                ipo.open_date,
                ipo.close_date,
                ipo.allotment_date,
                ipo.listing_date,
                ipo.status.as_str(),
                ipo.drhp_url,
                ipo.rhp_url,
                ipo.source,
                ipo.ingested_at,
                ipo.updated_at,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_ipo(&self, ipo: &Ipo) -> Result<(), DbError> {
        let id = ipo
            .id
            .ok_or_else(|| DbError::NotFound("IPO".into(), "missing id".into()))?;
        let conn = self.writer.lock().unwrap();
        let rows = conn.execute(
            "UPDATE ipos SET market_id=?1, company_name=?2, symbol=?3, exchange=?4,
             sector=?5, offer_type=?6, price_band_low=?7, price_band_high=?8,
             final_price=?9, lot_size=?10, shares_offered=?11, fresh_issue_shares=?12,
             ofs_shares=?13, shares_outstanding_post=?14, issue_size=?15,
             open_date=?16, close_date=?17, allotment_date=?18, listing_date=?19,
             status=?20, drhp_url=?21, rhp_url=?22, source=?23, ingested_at=?24,
             updated_at=?25
             WHERE id=?26",
            params![
                ipo.market_id,
                ipo.company_name,
                ipo.symbol,
                ipo.exchange,
                ipo.sector,
                ipo.offer_type,
                decimal_to_f64_opt(ipo.price_band_low),
                decimal_to_f64_opt(ipo.price_band_high),
                decimal_to_f64_opt(ipo.final_price),
                ipo.lot_size,
                ipo.shares_offered,
                ipo.fresh_issue_shares,
                ipo.ofs_shares,
                ipo.shares_outstanding_post,
                decimal_to_f64_opt(ipo.issue_size),
                ipo.open_date,
                ipo.close_date,
                ipo.allotment_date,
                ipo.listing_date,
                ipo.status.as_str(),
                ipo.drhp_url,
                ipo.rhp_url,
                ipo.source,
                ipo.ingested_at,
                ipo.updated_at,
                id,
            ],
        )?;
        if rows == 0 {
            return Err(DbError::ipo_not_found(id));
        }
        Ok(())
    }

    pub fn upsert_ipo_by_source(&self, ipo: &Ipo) -> Result<i64, DbError> {
        let conn = self.writer.lock().unwrap();
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM ipos WHERE source=?1 AND company_name=?2",
                params![ipo.source, ipo.company_name],
                |row| row.get(0),
            )
            .ok();

        if let Some(existing_id) = existing {
            let mut updated = ipo.clone();
            updated.id = Some(existing_id);
            drop(conn);
            self.update_ipo(&updated)?;
            Ok(existing_id)
        } else {
            drop(conn);
            self.insert_ipo(ipo)
        }
    }

    pub fn get_ipo(&self, id: i64) -> Result<Option<Ipo>, DbError> {
        self.reader
            .query_row(
                "SELECT id, market_id, company_name, symbol, exchange, sector, offer_type,
                 price_band_low, price_band_high, final_price, lot_size, shares_offered,
                 fresh_issue_shares, ofs_shares, shares_outstanding_post, issue_size,
                 open_date, close_date, allotment_date, listing_date, status,
                 drhp_url, rhp_url, source, ingested_at, updated_at
                 FROM ipos WHERE id=?1",
                params![id],
                Self::row_to_ipo,
            )
            .map(Some)
            .or_else(|e| {
                if e == rusqlite::Error::QueryReturnedNoRows {
                    Ok(None)
                } else {
                    Err(DbError::from(e))
                }
            })
    }

    pub fn list_ipos(&self, status_filter: Option<&str>) -> Result<Vec<Ipo>, DbError> {
        let (sql, filter_params): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(f) = status_filter {
                (
                    "SELECT id, market_id, company_name, symbol, exchange, sector, offer_type,
                     price_band_low, price_band_high, final_price, lot_size, shares_offered,
                     fresh_issue_shares, ofs_shares, shares_outstanding_post, issue_size,
                     open_date, close_date, allotment_date, listing_date, status,
                     drhp_url, rhp_url, source, ingested_at, updated_at
                     FROM ipos WHERE status=?1 ORDER BY open_date DESC",
                    vec![Box::new(f.to_owned())],
                )
            } else {
                (
                    "SELECT id, market_id, company_name, symbol, exchange, sector, offer_type,
                     price_band_low, price_band_high, final_price, lot_size, shares_offered,
                     fresh_issue_shares, ofs_shares, shares_outstanding_post, issue_size,
                     open_date, close_date, allotment_date, listing_date, status,
                     drhp_url, rhp_url, source, ingested_at, updated_at
                     FROM ipos ORDER BY open_date DESC",
                    vec![],
                )
            };

        let mut stmt = self.reader.prepare(sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> =
            filter_params.iter().map(|p| p.as_ref()).collect();
        let rows = stmt.query_map(params.as_slice(), Self::row_to_ipo)?;

        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn search_ipos(&self, query: &str) -> Result<Vec<Ipo>, DbError> {
        let pattern = format!("%{}%", query);
        let mut stmt = self.reader.prepare(
            "SELECT id, market_id, company_name, symbol, exchange, sector, offer_type,
             price_band_low, price_band_high, final_price, lot_size, shares_offered,
             fresh_issue_shares, ofs_shares, shares_outstanding_post, issue_size,
             open_date, close_date, allotment_date, listing_date, status,
             drhp_url, rhp_url, source, ingested_at, updated_at
             FROM ipos WHERE company_name LIKE ?1 ORDER BY open_date DESC",
        )?;
        let rows = stmt.query_map(params![pattern], Self::row_to_ipo)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ── Markets ──

    pub fn list_markets(&self) -> Result<Vec<Market>, DbError> {
        let mut stmt = self.reader.prepare("SELECT id, name, currency, currency_symbol FROM markets ORDER BY id")?;
        let rows = stmt.query_map([], |row| {
            Ok(Market {
                id: row.get(0)?,
                name: row.get(1)?,
                currency: row.get(2)?,
                currency_symbol: row.get(3)?,
            })
        })?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn get_market(&self, id: &str) -> Result<Option<Market>, DbError> {
        self.reader
            .query_row(
                "SELECT id, name, currency, currency_symbol FROM markets WHERE id=?1",
                params![id],
                |row| {
                    Ok(Market {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        currency: row.get(2)?,
                        currency_symbol: row.get(3)?,
                    })
                },
            )
            .map(Some)
            .or_else(|e| {
                if e == rusqlite::Error::QueryReturnedNoRows {
                    Ok(None)
                } else {
                    Err(DbError::from(e))
                }
            })
    }

    // ── Subscriptions ──

    pub fn insert_subscription_snapshot(&self, entry: &SubscriptionEntry) -> Result<i64, DbError> {
        let conn = self.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO subscription_snapshots (ipo_id, snapshot_at, category, subscribed, source, ingested_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                entry.ipo_id,
                entry.snapshot_at,
                entry.category,
                decimal_to_f64_opt(entry.subscribed),
                entry.source,
                entry.ingested_at,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_subscriptions_for_ipo(&self, ipo_id: i64) -> Result<Vec<SubscriptionEntry>, DbError> {
        let mut stmt = self.reader.prepare(
            "SELECT id, ipo_id, snapshot_at, category, subscribed, source, ingested_at
             FROM subscription_snapshots WHERE ipo_id=?1 ORDER BY snapshot_at, category",
        )?;
        let rows = stmt.query_map(params![ipo_id], Self::row_to_subscription)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    pub fn get_latest_snapshot(&self, ipo_id: i64) -> Result<Vec<SubscriptionEntry>, DbError> {
        let mut stmt = self.reader.prepare(
            "SELECT id, ipo_id, snapshot_at, category, subscribed, source, ingested_at
             FROM subscription_snapshots
             WHERE ipo_id=?1 AND snapshot_at = (
                 SELECT MAX(snapshot_at) FROM subscription_snapshots WHERE ipo_id=?1
             )
             ORDER BY category",
        )?;
        let rows = stmt.query_map(params![ipo_id], Self::row_to_subscription)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ── Price History ──

    pub fn insert_price_point(&self, point: &PricePoint) -> Result<i64, DbError> {
        let conn = self.writer.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO price_history (ipo_id, trade_date, open_price, high_price, low_price, close_price, volume, source, ingested_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                point.ipo_id,
                point.trade_date,
                decimal_to_f64_opt(point.open_price),
                decimal_to_f64_opt(point.high_price),
                decimal_to_f64_opt(point.low_price),
                decimal_to_f64_opt(point.close_price),
                point.volume,
                point.source,
                point.ingested_at,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_price_history(&self, ipo_id: i64) -> Result<Vec<PricePoint>, DbError> {
        let mut stmt = self.reader.prepare(
            "SELECT id, ipo_id, trade_date, open_price, high_price, low_price, close_price, volume, source, ingested_at
             FROM price_history WHERE ipo_id=?1 ORDER BY trade_date",
        )?;
        let rows = stmt.query_map(params![ipo_id], Self::row_to_price_point)?;
        let mut result = Vec::new();
        for row in rows {
            result.push(row?);
        }
        Ok(result)
    }

    // ── KVP Store ──

    pub fn kv_get(&self, key: &str) -> Result<Option<String>, DbError> {
        self.reader
            .query_row(
                "SELECT value FROM key_value_store WHERE key=?1",
                params![key],
                |row| row.get(0),
            )
            .map(Some)
            .or_else(|e| {
                if e == rusqlite::Error::QueryReturnedNoRows {
                    Ok(None)
                } else {
                    Err(DbError::from(e))
                }
            })
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<(), DbError> {
        let conn = self.writer.lock().unwrap();
        conn.execute(
            "INSERT INTO key_value_store (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ── Ingestion Runs ──

    pub fn start_ingestion_run(&self, source: &str) -> Result<i64, DbError> {
        let conn = self.writer.lock().unwrap();
        let now = jiff::Zoned::now().to_string();
        conn.execute(
            "INSERT INTO ingestion_runs (source, started_at, status) VALUES (?1, ?2, 'running')",
            params![source, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn finish_ingestion_run(
        &self,
        id: i64,
        status: &str,
        records_written: i64,
        notes: &str,
    ) -> Result<(), DbError> {
        let conn = self.writer.lock().unwrap();
        let now = jiff::Zoned::now().to_string();
        conn.execute(
            "UPDATE ingestion_runs SET finished_at=?1, status=?2, records_written=?3, notes=?4 WHERE id=?5",
            params![now, status, records_written, notes, id],
        )?;
        Ok(())
    }

    // ── Row parsers ──

    fn row_to_ipo(row: &rusqlite::Row) -> rusqlite::Result<Ipo> {
        let status_str: String = row.get(20)?;
        Ok(Ipo {
            id: Some(row.get(0)?),
            market_id: row.get(1)?,
            company_name: row.get(2)?,
            symbol: row.get(3)?,
            exchange: row.get(4)?,
            sector: row.get(5)?,
            offer_type: row.get(6)?,
            price_band_low: f64_to_decimal_opt(row.get(7)?),
            price_band_high: f64_to_decimal_opt(row.get(8)?),
            final_price: f64_to_decimal_opt(row.get(9)?),
            lot_size: row.get(10)?,
            shares_offered: row.get(11)?,
            fresh_issue_shares: row.get(12)?,
            ofs_shares: row.get(13)?,
            shares_outstanding_post: row.get(14)?,
            issue_size: f64_to_decimal_opt(row.get(15)?),
            open_date: row.get(16)?,
            close_date: row.get(17)?,
            allotment_date: row.get(18)?,
            listing_date: row.get(19)?,
            status: IpoStatus::from_str(&status_str).unwrap_or(IpoStatus::Upcoming),
            drhp_url: row.get(21)?,
            rhp_url: row.get(22)?,
            source: row.get(23)?,
            ingested_at: row.get(24)?,
            updated_at: row.get(25)?,
        })
    }

    fn row_to_subscription(row: &rusqlite::Row) -> rusqlite::Result<SubscriptionEntry> {
        Ok(SubscriptionEntry {
            id: Some(row.get(0)?),
            ipo_id: row.get(1)?,
            snapshot_at: row.get(2)?,
            category: row.get(3)?,
            subscribed: f64_to_decimal_opt(row.get(4)?),
            source: row.get(5)?,
            ingested_at: row.get(6)?,
        })
    }

    fn row_to_price_point(row: &rusqlite::Row) -> rusqlite::Result<PricePoint> {
        Ok(PricePoint {
            id: Some(row.get(0)?),
            ipo_id: row.get(1)?,
            trade_date: row.get(2)?,
            open_price: f64_to_decimal_opt(row.get(3)?),
            high_price: f64_to_decimal_opt(row.get(4)?),
            low_price: f64_to_decimal_opt(row.get(5)?),
            close_price: f64_to_decimal_opt(row.get(6)?),
            volume: row.get(7)?,
            source: row.get(8)?,
            ingested_at: row.get(9)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_db() -> MosaicDb {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!("mosaic_test_{id}.db"));
        let db = MosaicDb::open(&path).unwrap();
        std::fs::remove_file(&path).ok();
        db
    }

    fn sample_ipo() -> Ipo {
        Ipo {
            id: None,
            market_id: "in".into(),
            company_name: "Test Corp".into(),
            symbol: None,
            exchange: Some("NSE".into()),
            sector: Some("Technology".into()),
            offer_type: Some("fresh_issue".into()),
            price_band_low: Some(Decimal::new(100, 0)),
            price_band_high: Some(Decimal::new(120, 0)),
            final_price: None,
            lot_size: Some(50),
            shares_offered: Some(1_000_000),
            fresh_issue_shares: Some(800_000),
            ofs_shares: Some(200_000),
            shares_outstanding_post: None,
            issue_size: Some(Decimal::new(120_000_000, 0)),
            open_date: Some("2026-07-15".into()),
            close_date: Some("2026-07-17".into()),
            allotment_date: None,
            listing_date: None,
            status: IpoStatus::Upcoming,
            drhp_url: None,
            rhp_url: None,
            source: "test".into(),
            ingested_at: "2026-07-01T00:00:00Z".into(),
            updated_at: "2026-07-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn test_insert_and_get_ipo() {
        let db = test_db();
        let id = db.insert_ipo(&sample_ipo()).unwrap();
        assert!(id > 0);

        let fetched = db.get_ipo(id).unwrap().expect("should exist");
        assert_eq!(fetched.company_name, "Test Corp");
        assert_eq!(fetched.status, IpoStatus::Upcoming);
        assert_eq!(fetched.price_band_low.unwrap(), Decimal::new(100, 0));
    }

    #[test]
    fn test_update_ipo() {
        let db = test_db();
        let id = db.insert_ipo(&sample_ipo()).unwrap();

        let mut ipo = sample_ipo();
        ipo.id = Some(id);
        ipo.status = IpoStatus::Open;
        ipo.final_price = Some(Decimal::new(110, 0));
        db.update_ipo(&ipo).unwrap();

        let fetched = db.get_ipo(id).unwrap().unwrap();
        assert_eq!(fetched.status, IpoStatus::Open);
        assert_eq!(
            fetched.final_price.unwrap(),
            Decimal::new(110, 0)
        );
    }

    #[test]
    fn test_upsert() {
        let db = test_db();
        let id1 = db.upsert_ipo_by_source(&sample_ipo()).unwrap();

        let mut same = sample_ipo();
        same.final_price = Some(Decimal::new(115, 0));
        let id2 = db.upsert_ipo_by_source(&same).unwrap();

        assert_eq!(id1, id2);
        let fetched = db.get_ipo(id1).unwrap().unwrap();
        assert_eq!(fetched.final_price.unwrap(), Decimal::new(115, 0));
    }

    #[test]
    fn test_list_filter() {
        let db = test_db();
        db.insert_ipo(&sample_ipo()).unwrap();

        let mut ipo2 = sample_ipo();
        ipo2.company_name = "Another Corp".into();
        ipo2.status = IpoStatus::Listed;
        db.insert_ipo(&ipo2).unwrap();

        let all = db.list_ipos(None).unwrap();
        assert_eq!(all.len(), 2);

        let listed = db.list_ipos(Some("listed")).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].company_name, "Another Corp");
    }

    #[test]
    fn test_search() {
        let db = test_db();
        db.insert_ipo(&sample_ipo()).unwrap();

        let mut other = sample_ipo();
        other.company_name = "Acme Industries".into();
        db.insert_ipo(&other).unwrap();

        let result = db.search_ipos("Test").unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].company_name, "Test Corp");
    }

    #[test]
    fn test_get_market() {
        let db = test_db();
        let market = db.get_market("in").unwrap().unwrap();
        assert_eq!(market.currency_symbol, "\u{20b9}");
    }

    #[test]
    fn test_subscriptions() {
        let db = test_db();
        let ipo_id = db.insert_ipo(&sample_ipo()).unwrap();

        let entry = SubscriptionEntry {
            id: None,
            ipo_id,
            snapshot_at: "2026-07-16T12:00:00Z".into(),
            category: "qib".into(),
            subscribed: Some(Decimal::new(35, 1)),
            source: "test".into(),
            ingested_at: "2026-07-16T12:00:00Z".into(),
        };
        db.insert_subscription_snapshot(&entry).unwrap();

        let entries = db.get_subscriptions_for_ipo(ipo_id).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_price_history() {
        let db = test_db();
        let ipo_id = db.insert_ipo(&sample_ipo()).unwrap();

        let point = PricePoint {
            id: None,
            ipo_id,
            trade_date: "2026-07-20".into(),
            open_price: Some(Decimal::new(130, 0)),
            high_price: Some(Decimal::new(135, 0)),
            low_price: Some(Decimal::new(128, 0)),
            close_price: Some(Decimal::new(132, 0)),
            volume: Some(1_000_000),
            source: "test".into(),
            ingested_at: "2026-07-20T00:00:00Z".into(),
        };
        db.insert_price_point(&point).unwrap();

        let history = db.get_price_history(ipo_id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].close_price.unwrap(), Decimal::new(132, 0));
    }

    #[test]
    fn test_insert_duplicate_price_point_is_noop() {
        let db = test_db();
        let ipo_id = db.insert_ipo(&sample_ipo()).unwrap();

        let point = PricePoint {
            id: None,
            ipo_id,
            trade_date: "2026-07-20".into(),
            open_price: Some(Decimal::new(130, 0)),
            high_price: None,
            low_price: None,
            close_price: None,
            volume: None,
            source: "test".into(),
            ingested_at: "2026-07-20T00:00:00Z".into(),
        };
        db.insert_price_point(&point).unwrap();
        db.insert_price_point(&point).unwrap();

        let history = db.get_price_history(ipo_id).unwrap();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_kv_store() {
        let db = test_db();
        assert!(db.kv_get("theme").unwrap().is_none());

        db.kv_set("theme", "dark").unwrap();
        assert_eq!(db.kv_get("theme").unwrap().unwrap(), "dark");

        db.kv_set("theme", "light").unwrap();
        assert_eq!(db.kv_get("theme").unwrap().unwrap(), "light");
    }

    #[test]
    fn test_ingestion_runs() {
        let db = test_db();
        let id = db.start_ingestion_run("chittorgarh").unwrap();
        assert!(id > 0);

        db.finish_ingestion_run(id, "success", 10, "scraped 10 IPOs")
            .unwrap();
    }
}

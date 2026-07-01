use std::path::{Path, PathBuf};

use anyhow::Context as _;
use mosaic_core::db::MosaicDb;
use mosaic_core::scraper::IpoScraper;
use mosaic_core::types::{Ipo, SubscriptionEntry};
use mosaic_scrapers::chittorgarh::ChittorgarhScraper;

/// Summary of a sync run.
pub struct SyncResult {
    pub source: String,
    /// Number of IPOs returned by the scraper.
    pub total: usize,
    /// Number of IPOs whose data changed (IPO fields or subscriptions).
    pub updated: usize,
    /// Number of IPOs whose scraped data matched the DB (skipped).
    pub skipped: usize,
}

pub fn db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mosaic")
        .join("mosaic.db")
}

pub fn run_sync(db_path: &Path) -> Result<SyncResult, anyhow::Error> {
    let db = MosaicDb::open(db_path).context("failed to open DB for sync")?;
    let scraper = ChittorgarhScraper::new();
    let ipos = scraper.fetch_ipos()?;

    let run_id = db.start_ingestion_run("chittorgarh")?;
    let mut records = 0i64;

    let sync_result = (|| -> Result<(), anyhow::Error> {
        for ipo in &ipos {
            let existing = db.get_ipo_by_source("chittorgarh", &ipo.company_name)?;

            let db_id = if let Some(ref existing) = existing {
                if ipo_fields_equal(existing, ipo) {
                    existing.id.unwrap()
                } else {
                    let id = db.upsert_ipo_by_source(ipo)?;
                    records += 1;
                    id
                }
            } else {
                let id = db.upsert_ipo_by_source(ipo)?;
                records += 1;
                id
            };

            let entries = scraper.fetch_subscriptions(ipo)?;
            let latest = db.get_latest_snapshot(db_id)?;
            if subscriptions_differ(&latest, &entries) {
                for mut entry in entries {
                    entry.ipo_id = db_id;
                    db.insert_subscription_snapshot(&entry)?;
                }
            }
        }
        Ok(())
    })();

    let status = if sync_result.is_ok() { "success" } else { "failed" };
    let notes = match &sync_result {
        Err(e) => format!("{e:#}"),
        Ok(_) => String::new(),
    };
    db.finish_ingestion_run(run_id, status, records, &notes)?;

    if let Err(e) = db.wal_checkpoint() {
        log::warn!("WAL checkpoint failed: {e}");
    }

    sync_result?;

    let total = ipos.len();
    let updated = records as usize;
    let skipped = total - updated;

    Ok(SyncResult {
        source: "chittorgarh".into(),
        total,
        updated,
        skipped,
    })
}

fn ipo_fields_equal(a: &Ipo, b: &Ipo) -> bool {
    a.company_name == b.company_name
        && a.market_id == b.market_id
        && a.exchange == b.exchange
        && a.symbol == b.symbol
        && a.sector == b.sector
        && a.offer_type == b.offer_type
        && a.price_band_low == b.price_band_low
        && a.price_band_high == b.price_band_high
        && a.final_price == b.final_price
        && a.lot_size == b.lot_size
        && a.shares_offered == b.shares_offered
        && a.fresh_issue_shares == b.fresh_issue_shares
        && a.ofs_shares == b.ofs_shares
        && a.shares_outstanding_post == b.shares_outstanding_post
        && a.issue_size == b.issue_size
        && a.open_date == b.open_date
        && a.close_date == b.close_date
        && a.allotment_date == b.allotment_date
        && a.listing_date == b.listing_date
        && a.status == b.status
        && a.drhp_url == b.drhp_url
        && a.rhp_url == b.rhp_url
        && a.source == b.source
}

fn subscriptions_differ(latest: &[SubscriptionEntry], new: &[SubscriptionEntry]) -> bool {
    if new.is_empty() {
        return false;
    }
    if latest.is_empty() {
        return true;
    }

    let map: std::collections::HashMap<&str, &SubscriptionEntry> =
        latest.iter().map(|e| (e.category.as_str(), e)).collect();

    for entry in new {
        match map.get(entry.category.as_str()) {
            Some(latest_entry) if latest_entry.subscribed == entry.subscribed => {}
            _ => return true,
        }
    }

    for entry in latest {
        if !new.iter().any(|e| e.category == entry.category) {
            return true;
        }
    }

    false
}

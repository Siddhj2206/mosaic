use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context as _;
use gpui::*;
use mosaic_core::config::Config;
use mosaic_core::db::MosaicDb;
use mosaic_core::scraper::IpoScraper;
use mosaic_core::types::{Ipo, SubscriptionEntry};
use mosaic_scrapers::chittorgarh::ChittorgarhScraper;

struct MosaicApp {
    _db: MosaicDb,
    _sync_task: Task<()>,
}

fn db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("mosaic")
        .join("mosaic.db")
}

impl MosaicApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let path = db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let db = MosaicDb::open(&path).expect("Failed to open Mosaic database");
        let _sync_task = Self::schedule_sync(path, cx);
        Self {
            _db: db,
            _sync_task,
        }
    }

    fn schedule_sync(db_path: PathBuf, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let mut backoff = Duration::ZERO;
            loop {
                cx.background_executor().timer(backoff).await;

                let path = db_path.clone();
                let result = cx.background_spawn(async move {
                    run_sync(&path)
                })
                .await;

                match &result {
                    Ok(true) => {
                        log::info!("sync completed: data changed");
                        backoff = Duration::ZERO;
                    }
                    Ok(false) => {
                        log::debug!("sync completed: no changes");
                        backoff = Duration::ZERO;
                    }
                    Err(e) => {
                        log::error!("sync failed: {e:#}");
                        if backoff == Duration::ZERO {
                            backoff = Duration::from_secs(1);
                        } else {
                            backoff = std::cmp::min(backoff * 2, Duration::from_secs(60));
                        }
                        continue;
                    }
                }

                this.update(cx, |_, cx| cx.notify()).ok();

                let interval = Config::load()
                    .refresh_interval_secs
                    .unwrap_or(300);
                cx.background_executor()
                    .timer(Duration::from_secs(interval))
                    .await;
            }
        })
    }
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
    if latest.is_empty() {
        return !new.is_empty();
    }
    if new.is_empty() {
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

fn run_sync(db_path: &PathBuf) -> Result<bool, anyhow::Error> {
    let db = MosaicDb::open(db_path).context("failed to open DB for sync")?;
    let scraper = ChittorgarhScraper::new();
    let ipos = scraper.fetch_ipos()?;

    let run_id = db.start_ingestion_run("chittorgarh")?;
    let mut records = 0i64;
    let mut any_writes = false;

    let sync_result = (|| -> Result<(), anyhow::Error> {
        for ipo in &ipos {
            let existing = db.get_ipo_by_source("chittorgarh", &ipo.company_name)?;

            if let Some(ref existing) = existing {
                if ipo_fields_equal(existing, ipo) {
                    continue;
                }
            }

            let db_id = db.upsert_ipo_by_source(ipo)?;
            any_writes = true;
            records += 1;

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
    Ok(any_writes)
}

impl Render for MosaicApp {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().child("Mosaic IPO Tracker")
    }
}

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .init();

    Application::new().run(|cx: &mut App| {
        cx.open_window(WindowOptions::default(), |_window, cx| {
            cx.new(|cx| MosaicApp::new(cx))
        })
        .ok();
    });
}

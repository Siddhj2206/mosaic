//! The background sync engine: runs the scrapers against the DB (ADR-0005).

use std::path::Path;

use jiff::civil::Date;

use mosaic_core::db::crud::Conn;
use mosaic_core::{Db, Ipo, IpoScraper, IpoStatus, Result, RunStatus};
use mosaic_scrapers::{ChittorgarhScraper, IpoWatchScraper, NseScraper};

use crate::app::SyncTask;

/// What a sync run produced (for logging / future UI surfacing).
#[derive(Debug, Default)]
pub struct SyncOutcome {
    pub ipos: usize,
    pub subs: usize,
    pub prices: usize,
}

/// Run one sync task end-to-end: scrape → upsert → audit log.
pub fn run_sync(db_path: &Path, task: SyncTask, today: Date) -> Result<SyncOutcome> {
    let db = Db::open(db_path)?;
    let mut conn = db.writer()?;
    let mut outcome = SyncOutcome::default();

    match task {
        SyncTask::Calendar => {
            run_nse_calendar(&mut conn, today, &mut outcome);
            run_chittorgarh(&mut conn, today, &mut outcome);
            run_ipowatch(&mut conn, today, &mut outcome);
        }
        SyncTask::Subscription => {
            let reader = db.reader()?;
            let open_ipos = reader.list_ipos(Some(IpoStatus::Open), None)?;
            run_subscription_poll(&mut conn, &open_ipos, today, &mut outcome);
        }
        SyncTask::Eod => {
            let reader = db.reader()?;
            let listed = reader.list_ipos(Some(IpoStatus::Listed), None)?;
            run_eod_pull(&mut conn, &listed, today, &mut outcome);
        }
    }
    Ok(outcome)
}

fn run_nse_calendar(conn: &mut Conn, today: Date, outcome: &mut SyncOutcome) {
    let run_id = match conn.start_run("nse") {
        Ok(id) => id,
        Err(e) => {
            log::error!("start_run nse: {e}");
            return;
        }
    };
    match NseScraper::new().and_then(|mut s| s.fetch_ipos(today)) {
        Ok(ipos) => match conn.upsert_ipos(&ipos) {
            Ok(n) => {
                outcome.ipos += n;
                let _ = conn.finish_run(run_id, RunStatus::Success, n as i64, None);
            }
            Err(e) => {
                log::error!("upsert nse ipos: {e}");
                let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
            }
        },
        Err(e) => {
            log::warn!("NSE calendar failed: {e}");
            let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
        }
    }
}

fn run_chittorgarh(conn: &mut Conn, today: Date, outcome: &mut SyncOutcome) {
    let run_id = match conn.start_run("chittorgarh") {
        Ok(id) => id,
        Err(e) => {
            log::error!("start_run chittorgarh: {e}");
            return;
        }
    };
    match ChittorgarhScraper::new().and_then(|mut s| s.fetch_ipos(today)) {
        Ok(ipos) => match conn.upsert_ipos(&ipos) {
            Ok(n) => {
                outcome.ipos += n;
                let _ = conn.finish_run(run_id, RunStatus::Success, n as i64, None);
            }
            Err(e) => {
                log::error!("upsert chittorgarh ipos: {e}");
                let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
            }
        },
        Err(e) => {
            log::warn!("Chittorgarh calendar failed: {e}");
            let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
        }
    }
}

fn run_ipowatch(conn: &mut Conn, today: Date, outcome: &mut SyncOutcome) {
    let run_id = match conn.start_run("ipowatch") {
        Ok(id) => id,
        Err(e) => {
            log::error!("start_run ipowatch: {e}");
            return;
        }
    };
    match IpoWatchScraper::new().and_then(|mut s| s.fetch_ipos(today)) {
        Ok(ipos) => match conn.upsert_ipos(&ipos) {
            Ok(n) => {
                outcome.ipos += n;
                let _ = conn.finish_run(run_id, RunStatus::Success, n as i64, None);
            }
            Err(e) => {
                log::error!("upsert ipowatch ipos: {e}");
                let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
            }
        },
        Err(e) => {
            log::warn!("IPO Watch calendar failed: {e}");
            let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
        }
    }
}

/// Poll NSE for today's subscription snapshot of every open IPO.
fn run_subscription_poll(conn: &mut Conn, open_ipos: &[Ipo], _today: Date, outcome: &mut SyncOutcome) {
    let run_id = match conn.start_run("nse-subscription") {
        Ok(id) => id,
        Err(e) => {
            log::error!("start_run nse-subscription: {e}");
            return;
        }
    };
    let mut scraper = match NseScraper::new() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("NSE session failed: {e}");
            let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
            return;
        }
    };

    let mut written = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for ipo in open_ipos {
        if ipo.symbol.is_none() {
            continue;
        }
        match scraper.fetch_subscriptions(ipo) {
            Ok(snapshots) => match conn.upsert_subscription_snapshots(&snapshots) {
                Ok(n) => written += n,
                Err(e) => errors.push(format!("{}: db {e}", ipo.company_name)),
            },
            Err(e) => errors.push(format!("{}: {e}", ipo.company_name)),
        }
    }
    outcome.subs += written;
    let status = if errors.is_empty() {
        RunStatus::Success
    } else {
        RunStatus::Partial
    };
    let notes = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };
    let _ = conn.finish_run(run_id, status, written as i64, notes.as_deref());
}

/// Pull EOD history for every listed IPO (from listing date onward).
fn run_eod_pull(conn: &mut Conn, listed: &[Ipo], _today: Date, outcome: &mut SyncOutcome) {
    let run_id = match conn.start_run("nse-eod") {
        Ok(id) => id,
        Err(e) => {
            log::error!("start_run nse-eod: {e}");
            return;
        }
    };
    let mut scraper = match NseScraper::new() {
        Ok(s) => s,
        Err(e) => {
            log::warn!("NSE session failed: {e}");
            let _ = conn.finish_run(run_id, RunStatus::Failed, 0, Some(&e.to_string()));
            return;
        }
    };

    let mut written = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for ipo in listed {
        if ipo.symbol.is_none() {
            continue;
        }
        match scraper.fetch_price_history(ipo) {
            Ok(points) => match conn.upsert_price_history(&points) {
                Ok(n) => written += n,
                Err(e) => errors.push(format!("{}: db {e}", ipo.company_name)),
            },
            Err(e) => errors.push(format!("{}: {e}", ipo.company_name)),
        }
    }
    outcome.prices += written;
    let status = if errors.is_empty() {
        RunStatus::Success
    } else {
        RunStatus::Partial
    };
    let notes = if errors.is_empty() {
        None
    } else {
        Some(errors.join("; "))
    };
    let _ = conn.finish_run(run_id, status, written as i64, notes.as_deref());
}

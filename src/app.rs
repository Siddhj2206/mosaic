//! The `MosaicApp` root view: state, background sync loop, and pane
//! composition (ADR-0004, ADR-0005).

use std::collections::HashMap;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use gpui::{Context, ParentElement, Styled, Task, Window};
use jiff::civil::{Date, DateTime};
use jiff::tz::TimeZone;

use mosaic_core::db::crud::StatusCounts;
use mosaic_core::{Db, Ipo, IpoStatus, PricePoint, SubscriptionSnapshot};

use crate::sync::run_sync;
use crate::ui::dossier::Dossier;
use crate::ui::ipo_list::ListPane;
use crate::ui::sidebar::Sidebar;
use crate::ui::statusbar::StatusBar;
use crate::ui::titlebar::TitleBar;
use gpui::prelude::*;

pub struct MosaicApp {
    pub db: Db,

    // Data (refreshed from the reader connection)
    pub ipos: Vec<Ipo>,
    pub counts: StatusCounts,
    /// Latest snapshot per category, per IPO (for the list's inline rows).
    pub latest_sub: HashMap<i64, Vec<SubscriptionSnapshot>>,

    // Selection
    pub selected_ipo_id: Option<i64>,
    pub detail_subs: Vec<SubscriptionSnapshot>,
    pub detail_prices: Vec<PricePoint>,

    // List state
    pub status_filter: Option<IpoStatus>,
    pub search: String,
    pub sort_col: SortCol,
    pub sort_dir: SortDir,
    pub expanded_id: Option<i64>,

    // Dossier
    pub detail_tab: DetailTab,

    /// Search input state (created lazily on first render — needs a window).
    pub search_input: Option<gpui::Entity<gpui_component::input::InputState>>,
    /// Subscriptions that must stay alive for the app's lifetime.
    pub _subscriptions: Vec<gpui::Subscription>,

    // Sync
    pub is_syncing: bool,
    pub last_sync_at: Option<Instant>,
    pub last_sync_err: Option<String>,
    pub last_calendar_at: Option<DateTime>,
    pub last_sub_at: Option<DateTime>,
    pub last_eod_at: Option<DateTime>,
    sync_tx: Option<Sender<SyncTask>>,
    _sync_task: Task<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortCol {
    Name,
    Status,
    Band,
    Lot,
    IssueSize,
    Subscription,
    OpenDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortDir {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailTab {
    Overview,
    Subscription,
    Performance,
    Docs,
}

/// What the background loop should do on the next run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncTask {
    Calendar,
    Subscription,
    Eod,
}

impl MosaicApp {
    pub fn new(db: Db, cx: &mut Context<Self>) -> Self {
        let mut app = MosaicApp {
            db,
            ipos: Vec::new(),
            counts: StatusCounts::default(),
            latest_sub: HashMap::new(),
            selected_ipo_id: None,
            detail_subs: Vec::new(),
            detail_prices: Vec::new(),
            status_filter: None,
            search: String::new(),
            sort_col: SortCol::OpenDate,
            sort_dir: SortDir::Asc,
            expanded_id: None,
            detail_tab: DetailTab::Overview,
            search_input: None,
            _subscriptions: Vec::new(),
            is_syncing: false,
            last_sync_at: None,
            last_sync_err: None,
            last_calendar_at: None,
            last_sub_at: None,
            last_eod_at: None,
            sync_tx: None,
            _sync_task: cx.spawn(async move |_this, _cx| {}),
        };
        app.load_persisted_state();
        app.refresh_from_db();
        app.start_background_sync(cx);
        app
    }

    // -- persistence ------------------------------------------------------

    fn load_persisted_state(&mut self) {
        let Ok(conn) = self.db.reader() else { return };
        self.last_calendar_at = conn
            .kv_get("last_calendar_at")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok());
        self.last_sub_at = conn
            .kv_get("last_sub_at")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok());
        self.last_eod_at = conn
            .kv_get("last_eod_at")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok());
        if let Ok(Some(s)) = conn.kv_get("selected_ipo_id") {
            self.selected_ipo_id = s.parse().ok();
        }
    }

    // -- data refresh -----------------------------------------------------

    pub fn refresh_from_db(&mut self) {
        let Ok(conn) = self.db.reader() else {
            log::error!("db reader unavailable");
            return;
        };
        match conn.list_ipos(self.status_filter, Some(&self.search)) {
            Ok(mut ipos) => {
                sort_ipos(&mut ipos, self.sort_col, self.sort_dir);
                self.ipos = ipos;
            }
            Err(e) => log::error!("list_ipos: {e}"),
        }
        if let Ok(counts) = conn.status_counts() {
            self.counts = counts;
        }
        let mut latest = HashMap::new();
        for ipo in &self.ipos {
            if let Some(id) = ipo.id {
                if let Ok(rows) = conn.latest_subscription_by_category(id) {
                    latest.insert(id, rows);
                }
            }
        }
        self.latest_sub = latest;

        // Reload the selected IPO's detail data.
        let Some(id) = self.selected_ipo_id else { return };
        if !self.ipos.iter().any(|i| i.id == Some(id)) {
            // Selection fell out of the filtered list — keep it loaded anyway
            // if the IPO still exists in the DB.
            let exists = conn
                .list_ipos(None, None)
                .map(|all| all.iter().any(|i| i.id == Some(id)))
                .unwrap_or(false);
            if !exists {
                self.selected_ipo_id = None;
                self.detail_subs.clear();
                self.detail_prices.clear();
                return;
            }
        }
        self.detail_subs = conn.list_subscriptions(id).unwrap_or_default();
        self.detail_prices = conn.list_price_history(id).unwrap_or_default();
    }

    pub fn select_ipo(&mut self, id: Option<i64>, cx: &mut Context<Self>) {
        self.selected_ipo_id = id;
        self.refresh_from_db();
        if let Ok(mut w) = self.db.writer() {
            let _ = w.kv_set(
                "selected_ipo_id",
                &id.map(|i| i.to_string()).unwrap_or_default(),
            );
        }
        cx.notify();
    }

    // -- sync loop --------------------------------------------------------

    fn start_background_sync(&mut self, cx: &mut Context<Self>) {
        let weak = cx.entity().downgrade();
        let db_path = self.db.path().to_owned();
        let (tx, rx) = std::sync::mpsc::channel::<SyncTask>();
        self.sync_tx = Some(tx);

        let task = cx.spawn(async move |_this: gpui::WeakEntity<MosaicApp>, cx: &mut gpui::AsyncApp| {
            // Phase 0: immediate calendar run on launch.
            run_and_report(&weak, cx, &db_path, SyncTask::Calendar).await;

            let mut backoff = Duration::ZERO;
            loop {
                let wait = if backoff.is_zero() {
                    Duration::from_secs(60)
                } else {
                    backoff
                };
                match rx.recv_timeout(wait) {
                    Ok(task) => {
                        backoff = Duration::ZERO;
                        run_and_report(&weak, cx, &db_path, task).await;
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        let task = decide_by_clock(&weak, cx).await;
                        if let Some(task) = task {
                            let ok = run_and_report(&weak, cx, &db_path, task).await;
                            backoff = if ok {
                                Duration::ZERO
                            } else {
                                next_backoff(backoff)
                            };
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        self._sync_task = task;
    }

    /// Request an immediate sync of the given task type (manual refresh).
    pub fn request_sync(&self, task: SyncTask) {
        if let Some(tx) = &self.sync_tx {
            let _ = tx.send(task);
        }
    }
}

fn next_backoff(backoff: Duration) -> Duration {
    if backoff.is_zero() {
        Duration::from_secs(30)
    } else {
        (backoff * 2).min(Duration::from_secs(3600))
    }
}

/// Decide which task is due right now, from persisted last-run timestamps.
async fn decide_by_clock(
    weak: &gpui::WeakEntity<MosaicApp>,
    cx: &mut gpui::AsyncApp,
) -> Option<SyncTask> {
    let now = jiff::Zoned::now().datetime();
    let today = now.date();

    weak.update(cx, |this, _| {
        // Subscription poll: 18:00–18:59 local, once per day.
        if now.hour() == 18 && this.last_sub_at.map(|d| d.date()) != Some(today) {
            return Some(SyncTask::Subscription);
        }
        // EOD pull: 19:30–19:59 local, once per day.
        if now.hour() == 19 && now.minute() >= 30 && this.last_eod_at.map(|d| d.date()) != Some(today)
        {
            return Some(SyncTask::Eod);
        }
        // Calendar: every 6h, or if never run today.
        let due = match this.last_calendar_at {
            Some(last) => {
                let elapsed = now - last;
                elapsed.get_hours() >= 6
            }
            None => true,
        };
        if due && this.last_calendar_at.map(|d| d.date()) != Some(today) {
            return Some(SyncTask::Calendar);
        }
        None
    })
    .ok()
    .flatten()
}

/// Run one sync task in the background and report the outcome.
async fn run_and_report(
    weak: &gpui::WeakEntity<MosaicApp>,
    cx: &mut gpui::AsyncApp,
    db_path: &std::path::Path,
    task: SyncTask,
) -> bool {
    weak.update(cx, |this, cx| {
        this.is_syncing = true;
        cx.notify();
    })
    .ok();

    let today = Date::ZERO; // placeholder, replaced below
    let _ = today;
    let today = jiff::Zoned::now().with_time_zone(TimeZone::UTC).datetime().date();

    let result = cx
        .background_spawn({
            let db_path = db_path.to_owned();
            async move { run_sync(&db_path, task, today) }
        })
        .await;

    let ok = result.is_ok();
    weak.update(cx, |this, cx| {
        this.is_syncing = false;
        match result {
            Ok(outcome) => {
                this.last_sync_at = Some(Instant::now());
                this.last_sync_err = None;
                match task {
                    SyncTask::Calendar => this.last_calendar_at = Some(mosaic_core::types::now_utc()),
                    SyncTask::Subscription => this.last_sub_at = Some(mosaic_core::types::now_utc()),
                    SyncTask::Eod => this.last_eod_at = Some(mosaic_core::types::now_utc()),
                }
                let _ = outcome;
                this.persist_run_times();
            }
            Err(e) => {
                log::warn!("sync failed: {e}");
                this.last_sync_err = Some(e.to_string());
            }
        }
        this.refresh_from_db();
        cx.notify();
    })
    .ok();
    ok
}

impl MosaicApp {
    fn persist_run_times(&mut self) {
        let Ok(mut w) = self.db.writer() else { return };
        let _ = w.kv_set(
            "last_calendar_at",
            &self.last_calendar_at.map(|d| d.to_string()).unwrap_or_default(),
        );
        let _ = w.kv_set("last_sub_at", &self.last_sub_at.map(|d| d.to_string()).unwrap_or_default());
        let _ = w.kv_set("last_eod_at", &self.last_eod_at.map(|d| d.to_string()).unwrap_or_default());
    }
}

/// Sort IPOs in place by column/direction.
pub fn sort_ipos(ipos: &mut [Ipo], col: SortCol, dir: SortDir) {
    let cmp = |a: &Ipo, b: &Ipo| -> std::cmp::Ordering {
        use std::cmp::Ordering;
        let ord = match col {
            SortCol::Name => a.company_name.cmp(&b.company_name),
            SortCol::Status => a.status.as_str().cmp(b.status.as_str()),
            SortCol::Band => a.price_band_low.cmp(&b.price_band_low),
            SortCol::Lot => a.lot_size.cmp(&b.lot_size),
            SortCol::IssueSize => a.issue_size_cr.cmp(&b.issue_size_cr),
            SortCol::Subscription => Ordering::Equal, // filled in below
            SortCol::OpenDate => a.open_date.cmp(&b.open_date),
        };
        match dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    };
    ipos.sort_by(cmp);
}

// ---------------------------------------------------------------------------
// Render
// ---------------------------------------------------------------------------

impl gpui::Render for MosaicApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let palette = crate::theme::palette();
        let _ = cx;

        gpui::div()
            .flex()
            .flex_col()
            .size_full()
            .bg(palette.bg)
            .text_color(palette.text)
            .child(TitleBar::render(self, window, cx))
            .child(
                gpui::div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h(gpui::px(0.))
                    .child(Sidebar::render(self, cx))
                    .child(ListPane::render(self, window, cx))
                    .child(Dossier::render(self, window, cx)),
            )
            .child(StatusBar::render(self, cx))
    }
}

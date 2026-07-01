use std::path::PathBuf;
use std::time::Duration;

use gpui::*;
use mosaic_core::config::Config;
use mosaic_core::db::MosaicDb;

struct MosaicApp {
    _db: MosaicDb,
    _sync_task: Task<()>,
}

impl MosaicApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let path = mosaic::db_path();
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
                    mosaic::run_sync(&path)
                })
                .await;

                match &result {
                    Ok(result) => {
                        log::info!(
                            "sync completed: {} updated, {} skipped",
                            result.updated,
                            result.skipped
                        );
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

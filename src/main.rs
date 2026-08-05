//! Mosaic — deterministic India IPO tracker (v1).
//!
//! GPUI desktop app. Data syncs from NSE's official JSON APIs (primary) and
//! Chittorgarh/IPO Watch (archive) in the background; every record carries
//! `source` + `ingested_at` provenance.

mod app;
mod sync;
mod theme;
mod ui;

use gpui::{App, AppContext, Application, Bounds, Size, WindowBounds, WindowOptions, WindowKind};
use gpui::point;

use crate::app::MosaicApp;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let data_dir = dirs::data_dir()
        .map(|d| d.join("mosaic"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let db = match mosaic_core::Db::open(data_dir.join("mosaic.db")) {
        Ok(db) => db,
        Err(e) => {
            log::error!("failed to open database: {e}");
            eprintln!("mosaic: failed to open database at {}: {e}", data_dir.join("mosaic.db").display());
            std::process::exit(1);
        }
    };

    Application::new().run(move |cx: &mut App| {
        gpui_component::init(cx);
        cx.set_global(theme::palette());

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                point(px(80.), px(60.)),
                Size {
                    width: px(1440.),
                    height: px(900.),
                },
            ))),
            titlebar: Some(gpui::TitlebarOptions {
                title: Some("Mosaic".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            focus: true,
            show: true,
            kind: WindowKind::Normal,
            is_movable: true,
            is_resizable: true,
            is_minimizable: true,
            window_background: gpui::WindowBackgroundAppearance::Opaque,
            app_id: Some("mosaic".to_string()),
            window_min_size: Some(Size {
                width: px(960.),
                height: px(600.),
            }),
            ..Default::default()
        };

        let _ = cx.open_window(options, |window, cx| {
            let app: gpui::AnyView = cx
                .new(|cx| MosaicApp::new(db.clone(), cx))
                .into();
            // gpui-component components (Input) require the window root to
            // be gpui_component's Root.
            cx.new(|cx| gpui_component::Root::new(app, window, cx))
        });
    });
}

fn px(v: f32) -> gpui::Pixels {
    gpui::px(v)
}

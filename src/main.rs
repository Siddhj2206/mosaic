mod app;
mod ipo_detail;
mod ipo_list;

use app::{MosaicApp, SyncNow};
use gpui::*;

fn main() {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info"),
    )
    .init();

    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-r", SyncNow, Some("MosaicApp")),
            KeyBinding::new("ctrl-r", SyncNow, Some("MosaicApp")),
        ]);
        cx.open_window(WindowOptions::default(), |window, cx| {
            let view = cx.new(|cx| MosaicApp::new(window, cx));
            cx.new(|cx| gpui_component::Root::new(view, window, cx))
        })
        .ok();
    });
}

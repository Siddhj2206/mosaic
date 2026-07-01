use gpui::*;

struct MosaicApp;

impl MosaicApp {
    pub fn new(_: &mut Context<Self>) -> Self {
        Self
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

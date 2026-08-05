//! Title bar (34px, Zed-style): app name, search input, sync indicator,
//! refresh button.

use gpui::{Context, Styled, div, px};
use gpui::prelude::*;
use gpui_component::Sizable;
use gpui_component::input::{Input, InputState};
use gpui_component::input::InputEvent as ComponentInputEvent;
use gpui_component::Size;

use crate::app::{MosaicApp, SyncTask};
use crate::theme::{S1, S2, TITLEBAR_H};

pub struct TitleBar;

impl TitleBar {
    pub fn render(
        app: &mut MosaicApp,
        window: &mut gpui::Window,
        cx: &mut Context<MosaicApp>,
    ) -> gpui::Div {
        let palette = crate::theme::palette();
        ensure_search_input(app, window, cx);

        div()
            .h(px(TITLEBAR_H))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(S2))
            .gap(px(S2))
            .bg(palette.bg)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S2))
                    .child(
                        div()
                            .text_size(px(crate::theme::FONT_SM))
                            .text_color(palette.text_muted)
                            .child("Mosaic"),
                    )
                    .child(div().w(px(1.)).h(px(16.)).bg(palette.border_variant))
                    .child(search_input(app, window, cx)),
            )
            .child(sync_controls(app, cx))
    }
}

/// The search field: a gpui-component Input with Zed-like chrome (element
/// background, 6px radius, 1px border, focus border) driven by `app.search`.
fn search_input(app: &mut MosaicApp, _window: &mut gpui::Window, _cx: &mut Context<MosaicApp>) -> gpui::Div {
    let state = app.search_input.as_ref().expect("search input ensured");
    let palette = crate::theme::palette();
    let input = Input::new(state)
        .with_size(Size::Small)
        .cleanable(true)
        .bordered(false)
        .appearance(false)
        .w(px(240.));

    div()
        .h(px(crate::theme::INPUT_H))
        .flex()
        .flex_row()
        .items_center()
        .px(px(S1))
        .bg(palette.element)
        .rounded(px(crate::theme::RADIUS_MD))
        .border_1()
        .border_color(palette.border_variant)
        .child(input)
}

/// Create the search input state once (needs a window) and wire it up.
pub fn ensure_search_input(
    app: &mut MosaicApp,
    window: &mut gpui::Window,
    cx: &mut Context<MosaicApp>,
) {
    if app.search_input.is_some() {
        return;
    }
    let state = cx.new(|cx| {
        let mut s = InputState::new(window, cx);
        s.set_placeholder("Search IPOs…", window, cx);
        s
    });
    let sub = cx.subscribe(&state, |app, state, event, cx| {
        if let ComponentInputEvent::Change = event {
            app.search = state.read(cx).text().to_string();
            app.refresh_from_db();
            cx.notify();
        }
    });
    app.search_input = Some(state);
    app._subscriptions.push(sub);
}

fn sync_controls(app: &mut MosaicApp, cx: &mut Context<MosaicApp>) -> gpui::Div {
    let palette = crate::theme::palette();

    let status_text = if app.is_syncing {
        "Syncing…".to_string()
    } else if let Some(err) = &app.last_sync_err {
        format!("⚠ {err}")
    } else if let Some(at) = app.last_sync_at {
        let secs = at.elapsed().as_secs();
        if secs < 90 {
            format!("Synced {secs}s ago")
        } else {
            format!("Synced {}m ago", secs / 60)
        }
    } else {
        "Not synced yet".to_string()
    };

    let color = if app.is_syncing {
        palette.text_accent
    } else if app.last_sync_err.is_some() {
        palette.error
    } else {
        palette.text_muted
    };

    let refresh = div()
        .id(gpui::ElementId::Name("refresh".into()))
        .h(px(crate::theme::BUTTON_SM_H))
        .px(px(S1))
        .rounded(px(crate::theme::RADIUS_SM))
        .hover(|this| this.bg(palette.hover))
        .active(|this| this.bg(palette.active))
        .cursor_pointer()
        .on_click(cx.listener(|app, _: &gpui::ClickEvent, _, cx| {
            app.request_sync(SyncTask::Calendar);
            cx.notify();
        }))
        .child(
            div()
                .text_size(px(crate::theme::FONT_SM))
                .text_color(palette.text)
                .child("⟳ Refresh"),
        );

    div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S2))
        .child(
            div()
                .text_size(px(crate::theme::FONT_SM))
                .text_color(color)
                .child(status_text),
        )
        .child(refresh)
}

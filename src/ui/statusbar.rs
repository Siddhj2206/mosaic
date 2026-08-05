//! Status bar (Zed-style, ~26px): source status + provenance line.

use gpui::{Context, Styled, div, px};

use crate::app::MosaicApp;
use crate::theme::{S1, STATUSBAR_H};
use gpui::prelude::*;

pub struct StatusBar;

impl StatusBar {
    pub fn render(app: &mut MosaicApp, cx: &mut Context<MosaicApp>) -> gpui::Div {
        let _ = cx;
        let palette = crate::theme::palette();

        let left = if app.is_syncing {
            "⟳ syncing…".to_string()
        } else if let Some(err) = &app.last_sync_err {
            format!("⚠ {err}")
        } else {
            match (app.last_calendar_at, app.last_sub_at, app.last_eod_at) {
                (Some(c), s, e) => {
                    let s = s.map(|d| d.date()).unwrap_or(jiff::civil::Date::ZERO);
                    let e = e.map(|d| d.date()).unwrap_or(jiff::civil::Date::ZERO);
                    let _ = (s, e);
                    format!("calendar {} · NSE + Chittorgarh + IPO Watch", c.date())
                }
                _ => "not synced".to_string(),
            }
        };

        div()
            .h(px(STATUSBAR_H))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px(px(S1))
            .bg(palette.bg)
            .border_t_1()
            .border_color(palette.border)
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(palette.text_disabled)
                    .child(left),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(palette.text_disabled)
                    .child("v0.1.0 · India mainboard IPOs · deterministic"),
            )
    }
}

//! Left sidebar: navigation over the dossier sections + sync status at the
//! bottom (ADR-0004).

use gpui::{Context, InteractiveElement, Styled, div, px};

use crate::app::{DetailTab, MosaicApp};
use crate::theme::{S1, S2, S3, RADIUS_SM, FONT_SM};
use gpui::prelude::*;

pub struct Sidebar;

impl Sidebar {
    pub fn render(app: &mut MosaicApp, cx: &mut Context<MosaicApp>) -> gpui::Div {
        let palette = crate::theme::palette();

        let items: [(DetailTab, &str, &str); 5] = [
            (DetailTab::Overview, "Overview", "⧉"),
            (DetailTab::Subscription, "Subscription", "▤"),
            (DetailTab::Performance, "Performance", "▥"),
            (DetailTab::Docs, "Docs", "🗎"),
            (DetailTab::Overview, "IPO List", "☰"), // duplicates Overview; see note below
        ];

        div()
            .w(px(220.))
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.panel)
            .border_r_1()
            .border_color(palette.border)
            .child(nav_items(app, cx, &items))
            .child(sync_status(app))
    }
}

fn nav_items(
    app: &mut MosaicApp,
    cx: &mut Context<MosaicApp>,
    items: &[(DetailTab, &str, &str)],
) -> gpui::Div {
    let palette = crate::theme::palette();
    let mut col = div().flex().flex_col().p(px(S2)).gap(px(1.));

    let mut first = true;
    for (tab, label, icon) in items {
        let selected = !first && app.detail_tab == *tab;
        let _ = icon;
        let item = div()
            .id(gpui::ElementId::Name(label.to_string().into()))
            .h(px(28.))
            .px(px(S2))
            .rounded(px(RADIUS_SM))
            .flex()
            .flex_row()
            .items_center()
            .gap(px(S1))
            .text_size(px(FONT_SM))
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(palette.active).text_color(palette.text)
            })
            .when(!selected, |this| this.text_color(palette.text_muted))
            .hover(|this| {
                this.bg(palette.hover).text_color(palette.text)
            })
            .on_click({
                let tab = *tab;
                let label = label.to_string();
                cx.listener(move |app, _: &gpui::ClickEvent, _, cx| {
                    if label == "IPO List" {
                        // Focus the list: clear filter is not needed; just
                        // ensure selection stays and switch to Overview.
                        app.detail_tab = tab;
                    } else {
                        app.detail_tab = tab;
                    }
                    cx.notify();
                })
            })
            .child(div().child((*label).to_string()));
        col = col.child(item);
        first = false;
    }
    col
}

fn sync_status(app: &mut MosaicApp) -> gpui::Div {
    let palette = crate::theme::palette();
    let (text, color) = if app.is_syncing {
        ("⟳ Syncing…".to_string(), palette.text_accent)
    } else if let Some(err) = &app.last_sync_err {
        (format!("⚠ {err}"), palette.error)
    } else if let Some(at) = app.last_sync_at {
        let secs = at.elapsed().as_secs();
        (
            format!("✓ Synced {}m ago", secs / 60),
            palette.text_muted,
        )
    } else {
        ("Not synced".to_string(), palette.text_disabled)
    };

    div()
        .border_t_1()
        .border_color(palette.border)
        .p(px(S3))
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S1))
        .text_size(px(FONT_SM))
        .text_color(color)
        .child(text.to_string())
}

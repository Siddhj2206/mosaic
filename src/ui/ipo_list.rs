//! IPO list pane: stat strip, status pills, searchable/sortable dense table
//! with inline per-category subscription expansion (ADR-0004; Screener
//! pattern).

use gpui::{Context, InteractiveElement, Styled, div, px, IntoElement};

use mosaic_core::{Ipo, IpoStatus, SubCategory};

use crate::app::{MosaicApp, SortCol, SortDir};
use crate::theme::{S1, S2, FONT_SM, FONT_UI, ROW_H};
use crate::ui::format;
use crate::ui::status_badge;
use gpui::prelude::*;

pub struct ListPane;

impl ListPane {
    pub fn render(
        app: &mut MosaicApp,
        window: &mut gpui::Window,
        cx: &mut Context<MosaicApp>,
    ) -> gpui::Div {
        let _ = window;
        let palette = crate::theme::palette();

        div()
            .w(px(420.))
            .h_full()
            .flex()
            .flex_col()
            .bg(palette.panel)
            .border_r_1()
            .border_color(palette.border)
            .child(stat_strip(app))
            .child(pill_bar(app, cx))
            .child(table_header(app, cx))
            .child(table_rows(app, cx))
    }
}

// ---------------------------------------------------------------------------
// Stat strip (Groww/Chittorgarh pattern)
// ---------------------------------------------------------------------------

fn stat_strip(app: &MosaicApp) -> gpui::Div {
    let palette = crate::theme::palette();
    let c = &app.counts;

    let stat = |label: &str, n: i64, color: gpui::Hsla| {
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap(px(1.))
            .child(
                div()
                    .text_size(px(FONT_UI))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(color)
                    .child(n.to_string()),
            )
            .child(
                div()
                    .text_size(px(10.))
                    .text_color(palette.text_disabled)
                    .child(label.to_string()),
            )
    };

    div()
        .flex()
        .flex_row()
        .items_center()
        .justify_between()
        .px(px(S2))
        .py(px(S1))
        .border_b_1()
        .border_color(palette.border_variant)
        .child(stat("Upcoming", c.upcoming, palette.text_accent))
        .child(stat("Open", c.open, palette.success))
        .child(stat("Closed", c.closed, palette.warning))
        .child(stat("Listed", c.listed, palette.text))
}

// ---------------------------------------------------------------------------
// Filter pills
// ---------------------------------------------------------------------------

fn pill_bar(app: &mut MosaicApp, cx: &mut Context<MosaicApp>) -> gpui::Div {
    let palette = crate::theme::palette();
    let filters: [(Option<IpoStatus>, &str); 5] = [
        (None, "All"),
        (Some(IpoStatus::Upcoming), "Upcoming"),
        (Some(IpoStatus::Open), "Open"),
        (Some(IpoStatus::Closed), "Closed"),
        (Some(IpoStatus::Listed), "Listed"),
    ];

    let mut bar = div()
        .flex()
        .flex_row()
        .items_center()
        .gap(px(S1))
        .px(px(S2))
        .py(px(S1))
        .border_b_1()
        .border_color(palette.border_variant);

    for (status, label) in filters {
        let selected = app.status_filter == status;
        let pill = div()
            .id(gpui::ElementId::Name(format!("pill-{label}").into()))
            .h(px(20.))
            .px(px(S2))
            .rounded(px(9999.))
            .cursor_pointer()
            .when(selected, |this| this.bg(palette.active).text_color(palette.text))
            .when(!selected, |this| {
                this.text_color(palette.text_muted)
            })
            .hover(|this| {
                this.bg(palette.hover).text_color(palette.text)
            })
            .on_click({
                let status = status;
                cx.listener(move |app, _: &gpui::ClickEvent, _, cx| {
                    app.status_filter = status;
                    app.refresh_from_db();
                    cx.notify();
                })
            })
            .child(
                div()
                    .text_size(px(FONT_SM))
                    .child(label),
            );
        bar = bar.child(pill);
    }
    bar
}

// ---------------------------------------------------------------------------
// Table
// ---------------------------------------------------------------------------

const HEADERS: [(SortCol, &str, f32); 7] = [
    (SortCol::Name, "Company", 150.),
    (SortCol::Status, "Status", 70.),
    (SortCol::Band, "Band", 90.),
    (SortCol::Lot, "Lot", 50.),
    (SortCol::IssueSize, "Size (Cr)", 80.),
    (SortCol::Subscription, "Sub ×", 60.),
    (SortCol::OpenDate, "Open → Close", 110.),
];

fn table_header(app: &mut MosaicApp, cx: &mut Context<MosaicApp>) -> gpui::Div {
    let palette = crate::theme::palette();
    let mut row = div()
        .flex()
        .flex_row()
        .items_center()
        .h(px(26.))
        .px(px(S2))
        .border_b_1()
        .border_color(palette.border_variant);

    for (col, label, width) in HEADERS {
        let sorted = app.sort_col == col;
        let arrow = match (sorted, app.sort_dir) {
            (true, SortDir::Asc) => " ↑",
            (true, SortDir::Desc) => " ↓",
            (false, _) => "",
        };
        let header = div()
            .id(gpui::ElementId::Name(format!("sort-{label}").into()))
            .w(px(width))
            .cursor_pointer()
            .text_size(px(10.))
            .text_color(palette.text_disabled)
            .hover(|this| this.text_color(palette.text_muted))
            .on_click({
                let col = col;
                cx.listener(move |app, _: &gpui::ClickEvent, _, cx| {
                    if app.sort_col == col {
                        app.sort_dir = match app.sort_dir {
                            SortDir::Asc => SortDir::Desc,
                            SortDir::Desc => SortDir::Asc,
                        };
                    } else {
                        app.sort_col = col;
                        app.sort_dir = SortDir::Asc;
                    }
                    app.refresh_from_db();
                    cx.notify();
                })
            })
            .child(format!("{label}{arrow}"));
        row = row.child(header);
    }
    row
}

fn table_rows(app: &mut MosaicApp, cx: &mut Context<MosaicApp>) -> impl IntoElement {
    let palette = crate::theme::palette();

    let mut col = div()
        .id(gpui::ElementId::Name("ipo-list-scroll".into()))
        .flex_1()
        .flex()
        .flex_col()
        .overflow_y_scroll()
        .min_h(px(0.));

    if app.ipos.is_empty() {
        return col.child(crate::ui::empty_state(
            if app.search.is_empty() {
                "No IPOs match the current filter."
            } else {
                "No IPOs match the search."
            },
            &palette,
        ));
    }

    let mut rows: Vec<gpui::AnyElement> = app
        .ipos
        .iter()
        .enumerate()
        .map(|(idx, ipo)| row_for(app, cx, ipo, idx).into_any_element())
        .collect();
    let _ = &mut rows;

    for ipo in &app.ipos {
        let idx = app.ipos.iter().position(|i| i.id == ipo.id).unwrap_or(0);
        col = col.child(row_for(app, cx, ipo, idx));
    }
    col
}

fn row_for(
    app: &MosaicApp,
    cx: &mut Context<MosaicApp>,
    ipo: &Ipo,
    idx: usize,
) -> impl IntoElement {
    let palette = crate::theme::palette();
    let selected = app.selected_ipo_id == ipo.id;
    let expanded = app.expanded_id == ipo.id;

    let band = match (ipo.price_band_low, ipo.price_band_high) {
        (Some(l), Some(h)) => format!("{} – {}", format::rupees(l), format::rupees(h)),
        (Some(l), None) => format::rupees(l),
        _ => "—".to_string(),
    };
    let size = ipo
        .issue_size_cr
        .map(|d| format::rupees(d))
        .unwrap_or_else(|| "—".to_string());
    let sub = app
        .latest_sub
        .get(&ipo.id.unwrap_or(-1))
        .and_then(|rows| rows.iter().find(|r| r.category == SubCategory::Total))
        .and_then(|r| r.times_subscribed)
        .map(|d| format::times(d))
        .unwrap_or_else(|| "—".to_string());
    let window = match (ipo.open_date, ipo.close_date) {
        (Some(o), Some(c)) => format!("{} → {}", format::date(o), format::date(c)),
        (Some(o), None) => format::date(o),
        _ => "—".to_string(),
    };

    let chevron = if expanded { "▾" } else { "▸" };

    let chevron_click = div()
        .id(gpui::ElementId::Name(format!("ipo-chevron-{}", ipo.id.unwrap_or(idx as i64)).into()))
        .w(px(16.))
        .text_size(px(10.))
        .text_color(palette.text_disabled)
        .cursor_pointer()
        .on_click({
            let expanded_id = expanded;
            let id = ipo.id;
            cx.listener(move |app, _: &gpui::ClickEvent, _, cx| {
                app.expanded_id = if expanded_id { None } else { id };
                cx.notify();
            })
        })
        .child(chevron);

    let main_row = div()
        .h(px(ROW_H))
        .flex()
        .flex_row()
        .items_center()
        .px(px(S2))
        .gap(px(S1))
        .when(selected, |this| this.bg(palette.active))
        .when(!selected && idx % 2 == 1, |this| this.bg(palette.element))
        .hover(|this| this.bg(palette.hover))
        .child(chevron_click)
        .child(
            div()
                .w(px(134.))
                .truncate()
                .text_size(px(FONT_UI))
                .text_color(palette.text)
                .child(ipo.company_name.clone()),
        )
        .child(div().w(px(70.)).child(status_badge(ipo.status.as_str(), &palette)))
        .child(
            div()
                .w(px(90.))
                .truncate()
                .text_size(px(FONT_SM))
                .text_color(palette.text_muted)
                .child(band),
        )
        .child(
            div()
                .w(px(50.))
                .text_size(px(FONT_SM))
                .text_color(palette.text_muted)
                .child(ipo.lot_size.map(|l| l.to_string()).unwrap_or_else(|| "—".to_string())),
        )
        .child(
            div()
                .w(px(80.))
                .text_size(px(FONT_SM))
                .text_color(palette.text_muted)
                .child(size),
        )
        .child(
            div()
                .w(px(60.))
                .text_size(px(FONT_UI))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(palette.text)
                .child(sub),
        )
        .child(
            div()
                .w(px(110.))
                .truncate()
                .text_size(px(10.))
                .text_color(palette.text_disabled)
                .child(window),
        );

    // Selection click on the row body (not the chevron).
    let select_row = div()
        .id(gpui::ElementId::Name(format!("ipo-select-{}", ipo.id.unwrap_or(idx as i64)).into()))
        .h(px(ROW_H))
        .cursor_pointer()
        .on_click({
            let id = ipo.id;
            cx.listener(move |app, _: &gpui::ClickEvent, _, cx| {
                app.select_ipo(id, cx);
            })
        })
        .child(main_row);

    if expanded {
        let sub_rows: Vec<gpui::Div> = app
            .latest_sub
            .get(&ipo.id.unwrap_or(-1))
            .into_iter()
            .flatten()
            .map(|s| {
                let times = s
                    .times_subscribed
                    .map(|d| format::times(d))
                    .unwrap_or_else(|| "—".to_string());
                let offered = s
                    .offered_shares
                    .map(format::shares)
                    .unwrap_or_else(|| "—".to_string());
                let bid = s.bid_shares.map(format::shares).unwrap_or_else(|| "—".to_string());
                div()
                    .pl(px(24.))
                    .h(px(20.))
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(S2))
                    .text_size(px(10.))
                    .child(
                        div()
                            .w(px(60.))
                            .text_color(palette.text_muted)
                            .child(s.category.as_str().to_uppercase()),
                    )
                    .child(
                        div()
                            .w(px(50.))
                            .text_color(palette.text)
                            .child(times),
                    )
                    .child(
                        div()
                            .w(px(90.))
                            .text_color(palette.text_disabled)
                            .child(format!("offered {offered}")),
                    )
                    .child(
                        div()
                            .w(px(90.))
                            .text_color(palette.text_disabled)
                            .child(format!("bid {bid}")),
                    )
            })
            .collect();

        let mut expanded_block = div()
            .flex()
            .flex_col()
            .bg(palette.editor)
            .border_b_1()
            .border_color(palette.border_variant);
        for r in sub_rows {
            expanded_block = expanded_block.child(r);
        }
        div().flex().flex_col().child(select_row).child(expanded_block)
    } else {
        div().child(select_row)
    }
}

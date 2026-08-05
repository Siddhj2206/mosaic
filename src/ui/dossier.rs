//! Dossier pane: the per-IPO detail view with tabs (ADR-0004).
//!
//! - Overview: hero line, key facts, timetable, issue split, doc links
//! - Subscription: category × day grid with progress bars
//! - Performance: close-price line chart vs issue-price reference + OHLCV
//! - Docs: DRHP/RHP links

use gpui::{Context, InteractiveElement, IntoElement, Styled, canvas, div, px};

use rust_decimal::prelude::ToPrimitive;

use mosaic_core::{Ipo, SubCategory};

use crate::app::{DetailTab, MosaicApp};
use crate::theme::{S1, S2, S3, S4, ROW_H, RADIUS_SM, FONT_SM, FONT_UI};
use crate::ui::format;
use crate::ui::{empty_state, h_flex, kv_row, section_label, status_badge, v_flex};
use gpui::prelude::*;

pub struct Dossier;

impl Dossier {
    pub fn render(
        app: &mut MosaicApp,
        window: &mut gpui::Window,
        cx: &mut Context<MosaicApp>,
    ) -> gpui::Div {
        let _ = window;
        let palette = crate::theme::palette();

        let Some(ipo_id) = app.selected_ipo_id else {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .bg(palette.bg)
                .child(empty_state("Select an IPO to see its dossier", &palette));
        };
        let Some(ipo) = app.ipos.iter().find(|i| i.id == Some(ipo_id)).cloned() else {
            return div()
                .flex_1()
                .flex()
                .flex_col()
                .bg(palette.bg)
                .child(empty_state("Select an IPO to see its dossier", &palette));
        };

        div()
            .flex_1()
            .flex()
            .flex_col()
            .min_w(px(0.))
            .bg(palette.bg)
            .child(tab_bar(app, cx))
            .child(content(app, cx, &ipo))
    }
}

// ---------------------------------------------------------------------------
// Tabs
// ---------------------------------------------------------------------------

fn tab_bar(app: &mut MosaicApp, cx: &mut Context<MosaicApp>) -> gpui::Div {
    let palette = crate::theme::palette();
    let tabs: [(DetailTab, &str); 4] = [
        (DetailTab::Overview, "Overview"),
        (DetailTab::Subscription, "Subscription"),
        (DetailTab::Performance, "Performance"),
        (DetailTab::Docs, "Docs"),
    ];

    let mut bar = div()
        .h(px(crate::theme::TABBAR_H))
        .flex()
        .flex_row()
        .items_center()
        .px(px(S2))
        .gap(px(S1))
        .bg(palette.panel)
        .border_b_1()
        .border_color(palette.border);

    for (tab, label) in tabs {
        let selected = app.detail_tab == tab;
        let item = div()
            .id(gpui::ElementId::Name(format!("tab-{label}").into()))
            .h(px(crate::theme::TABBAR_H - 1.))
            .px(px(S2))
            .rounded(px(RADIUS_SM))
            .flex()
            .flex_row()
            .items_center()
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(palette.editor)
                    .text_color(palette.text)
                    .border_b_1()
                    .border_color(palette.border)
            })
            .when(!selected, |this| this.text_color(palette.text_muted))
            .hover(|this| this.bg(palette.hover).text_color(palette.text))
            .on_click({
                let tab = tab;
                cx.listener(move |app, _: &gpui::ClickEvent, _, cx| {
                    app.detail_tab = tab;
                    cx.notify();
                })
            })
            .child(div().text_size(px(FONT_SM)).child(label));
        bar = bar.child(item);
    }
    bar
}

// ---------------------------------------------------------------------------
// Content dispatch
// ---------------------------------------------------------------------------

fn content(app: &mut MosaicApp, cx: &mut Context<MosaicApp>, ipo: &Ipo) -> gpui::AnyElement {
    match app.detail_tab {
        DetailTab::Overview => overview(app, cx, ipo).into_any_element(),
        DetailTab::Subscription => subscription_view(app, ipo).into_any_element(),
        DetailTab::Performance => performance_view(app, ipo).into_any_element(),
        DetailTab::Docs => docs_view(app, ipo).into_any_element(),
    }
}

// ---------------------------------------------------------------------------
// Overview
// ---------------------------------------------------------------------------

fn overview(app: &mut MosaicApp, cx: &mut Context<MosaicApp>, ipo: &Ipo) -> impl IntoElement {
    let _ = cx;
    let palette = crate::theme::palette();

    let issue_size = ipo
        .issue_size_cr
        .map(|d| format!("{}", format::crores(d)))
        .unwrap_or_else(|| "—".to_string());
    let exchanges = ipo.exchange.as_deref().unwrap_or("NSE");

    // Hero line: "Bookbuilding IPO | ₹3,067 Cr | BSE, NSE | 10–12 Aug"
    let hero = v_flex()
        .gap(px(S1))
        .child(
            div()
                .text_size(px(crate::theme::FONT_LG))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(palette.text)
                .child(ipo.company_name.clone()),
        )
        .child(
            h_flex()
                .gap(px(S1))
                .child(status_badge(ipo.status.as_str(), &palette))
                .child(
                    div()
                        .text_size(px(FONT_SM))
                        .text_color(palette.text_muted)
                        .child(meta_line(ipo)),
                ),
        );

    // Key facts grid.
    let final_price = ipo
        .final_price
        .map(|d| format::rupees(d))
        .unwrap_or_else(|| "—".to_string());
    let lot = ipo
        .lot_size
        .map(|l| l.to_string())
        .unwrap_or_else(|| "—".to_string());
    let min_invest = match (ipo.final_price.or(ipo.price_band_high), ipo.lot_size) {
        (Some(p), Some(l)) => format::rupees(p * rust_decimal::Decimal::from(l as i64)),
        _ => "—".to_string(),
    };

    let facts = v_flex()
        .gap(px(2.))
        .child(kv_row("Price band", format!("{} – {}", band_low(ipo), band_high(ipo)), &palette))
        .child(kv_row("Final price", final_price, &palette))
        .child(kv_row("Face value", fmt_optional(ipo.face_value), &palette))
        .child(kv_row("Lot size", format!("{lot} shares"), &palette))
        .child(kv_row("Min investment", min_invest, &palette))
        .child(kv_row("Issue size", issue_size, &palette))
        .child(kv_row("Shares offered", ipo.shares_offered.map(format::shares).unwrap_or_else(|| "—".to_string()), &palette))
        .child(kv_row("Issue type", ipo.issue_type.clone().unwrap_or_else(|| "—".to_string()), &palette))
        .child(kv_row("Offer type", ipo.offer_type.clone().unwrap_or_else(|| "—".to_string()), &palette))
        .child(kv_row("Listing at", exchanges.to_string(), &palette));

    // Timetable pipeline.
    let timetable = v_flex()
        .gap(px(S1))
        .child(section_label("TIMETABLE", &palette))
        .child(
            h_flex()
                .gap(px(S1))
                .child(timetable_step("Open", ipo.open_date, palette.info, &palette))
                .child(div().text_color(palette.border).child("→"))
                .child(timetable_step("Close", ipo.close_date, palette.warning, &palette))
                .child(div().text_color(palette.border).child("→"))
                .child(timetable_step("Allotment", ipo.allotment_date, palette.text_muted, &palette))
                .child(div().text_color(palette.border).child("→"))
                .child(timetable_step("Listing", ipo.listing_date, palette.success, &palette)),
        );

    // Issue split.
    let split = v_flex()
        .gap(px(S1))
        .child(section_label("ISSUE SPLIT", &palette))
        .child(kv_row(
            "Fresh issue",
            ipo.fresh_issue_shares.map(format::shares).unwrap_or_else(|| "—".to_string()),
            &palette,
        ))
        .child(kv_row(
            "Offer for sale",
            ipo.ofs_shares.map(format::shares).unwrap_or_else(|| "—".to_string()),
            &palette,
        ));

    let _ = app;
    v_flex()
        .id(gpui::ElementId::Name("dossier-overview".into()))
        .flex_1()
        .overflow_y_scroll()
        .p(px(S3))
        .gap(px(S4))
        .child(hero)
        .child(facts)
        .child(timetable)
        .child(split)
}

fn meta_line(ipo: &Ipo) -> String {
    let kind = ipo.issue_type.as_deref().unwrap_or("IPO");
    let window = match (ipo.open_date, ipo.close_date) {
        (Some(o), Some(c)) => format!("{} – {}", format::date(o), format::date(c)),
        _ => "dates TBA".to_string(),
    };
    let exchanges = ipo.exchange.as_deref().unwrap_or("NSE");
    format!("{kind} | {exchanges} | {window}")
}

fn band_low(ipo: &Ipo) -> String {
    ipo.price_band_low.map(format::rupees).unwrap_or_else(|| "—".to_string())
}

fn band_high(ipo: &Ipo) -> String {
    ipo.price_band_high.map(format::rupees).unwrap_or_else(|| "—".to_string())
}

fn fmt_optional(d: Option<rust_decimal::Decimal>) -> String {
    d.map(format::rupees).unwrap_or_else(|| "—".to_string())
}

fn timetable_step(label: &str, date: Option<jiff::civil::Date>, color: gpui::Hsla, palette: &crate::theme::Palette) -> gpui::Div {
    let date_str = date.map(format::date).unwrap_or_else(|| "TBA".to_string());
    v_flex()
        .items_center()
        .gap(px(2.))
        .p(px(S1))
        .bg(palette.element)
        .rounded(px(RADIUS_SM))
        .child(div().text_size(px(10.)).text_color(palette.text_disabled).child(label.to_string()))
        .child(div().text_size(px(FONT_SM)).font_weight(gpui::FontWeight::MEDIUM).text_color(color).child(date_str))
}

// ---------------------------------------------------------------------------
// Subscription
// ---------------------------------------------------------------------------

fn subscription_view(app: &MosaicApp, ipo: &Ipo) -> impl IntoElement {
    let palette = crate::theme::palette();

    let days: Vec<jiff::civil::Date> = {
        let mut days: Vec<jiff::civil::Date> = app
            .detail_subs
            .iter()
            .map(|s| s.snapshot_at)
            .collect();
        days.sort();
        days.dedup();
        days
    };

    if days.is_empty() {
        return v_flex()
            .flex_1()
            .p(px(S3))
            .child(empty_state(
                "No subscription data yet — polls start when the window opens.",
                &palette,
            ));
    }

    let cats = [SubCategory::Qib, SubCategory::Nii, SubCategory::Retail, SubCategory::Total];

    // Grid header: blank corner + day columns.
    let mut grid = div()
        .flex()
        .flex_col()
        .p(px(S3))
        .gap(px(S1))
        .child(section_label(
            &format!("SUBSCRIPTION — {} polls", days.len()),
            &palette,
        ))
        .child(grid_header(&days, &palette));

    let max_times = app
        .detail_subs
        .iter()
        .filter_map(|s| s.times_subscribed)
        .fold(rust_decimal::Decimal::ZERO, |acc, t| {
            if t > acc { t } else { acc }
        });

    for cat in cats {
        let mut row = h_flex()
            .h(px(28.))
            .gap(px(S1))
            .child(
                div()
                    .w(px(90.))
                    .text_size(px(FONT_SM))
                    .text_color(palette.text)
                    .child(cat.as_str().to_uppercase()),
            );
        for day in &days {
            let cell = app
                .detail_subs
                .iter()
                .find(|s| s.category == cat && s.snapshot_at == *day);
            let value = cell
                .and_then(|s| s.times_subscribed)
                .map(|t| format::times(t))
                .unwrap_or_else(|| "–".to_string());
            let color = cell
                .and_then(|s| s.times_subscribed)
                .map(|t| {
                    if t >= rust_decimal::Decimal::ONE {
                        palette.up
                    } else {
                        palette.text_muted
                    }
                })
                .unwrap_or(palette.text_disabled);
            row = row.child(
                div()
                    .w(px(70.))
                    .text_size(px(FONT_UI))
                    .text_color(color)
                    .child(value),
            );
        }
        grid = grid.child(row);
    }

    // Progress bars (latest per category).
    let mut bars = v_flex().p(px(S3)).pt(px(0.)).gap(px(S1));
    for cat in cats {
        let latest = app
            .detail_subs
            .iter()
            .filter(|s| s.category == cat)
            .max_by_key(|s| s.snapshot_at)
            .and_then(|s| s.times_subscribed);
        let (frac, label) = match latest {
            Some(t) if max_times > rust_decimal::Decimal::ZERO => {
                let frac = t / max_times;
                let pct = (frac * rust_decimal::Decimal::from(100)).round_dp(0);
                (pct.to_f64().unwrap_or(0.0) as f32 / 100.0, format::times(t))
            }
            Some(t) => (0.0, format::times(t)),
            None => (0.0, "–".to_string()),
        };
        bars = bars.child(
            h_flex()
                .gap(px(S2))
                .child(
                    div()
                        .w(px(90.))
                        .text_size(px(FONT_SM))
                        .text_color(palette.text_muted)
                        .child(cat.as_str().to_uppercase()),
                )
                .child(
                    div()
                        .flex_1()
                        .h(px(6.))
                        .bg(palette.element)
                        .rounded(px(3.))
                        .child(
                            div()
                                .h_full()
                                .w(px(frac * 400.0))
                                .max_w(px(400.))
                                .bg(palette.info)
                                .rounded(px(3.)),
                        ),
                )
                .child(
                    div()
                        .w(px(60.))
                        .text_size(px(FONT_SM))
                        .text_color(palette.text)
                        .child(label),
                ),
        );
    }

    let _ = ipo;
    div().child(
        v_flex()
            .id(gpui::ElementId::Name("dossier-sub".into()))
            .flex_1()
            .overflow_y_scroll()
            .child(grid)
            .child(bars),
    )
}

fn grid_header(days: &[jiff::civil::Date], palette: &crate::theme::Palette) -> gpui::Div {
    let mut row = h_flex()
        .h(px(22.))
        .gap(px(S1))
        .child(div().w(px(90.)).text_size(px(10.)).text_color(palette.text_disabled).child("Category"));
    for day in days {
        row = row.child(
            div()
                .w(px(70.))
                .text_size(px(10.))
                .text_color(palette.text_disabled)
                .child(format::date(*day)),
        );
    }
    row
}

// ---------------------------------------------------------------------------
// Performance
// ---------------------------------------------------------------------------

fn performance_view(app: &MosaicApp, ipo: &Ipo) -> impl IntoElement {
    let palette = crate::theme::palette();

    if app.detail_prices.is_empty() {
        return v_flex()
            .flex_1()
            .p(px(S3))
            .child(empty_state(
                if ipo.status == mosaic_core::IpoStatus::Listed {
                    "No price history yet — EOD pulls start at 19:30."
                } else {
                    "Price history appears after listing."
                },
                &palette,
            ));
    }

    let prices = &app.detail_prices;
    let last_close = prices.last().and_then(|p| p.close_price);
    let issue_price = ipo.final_price.or(ipo.price_band_high);
    let listing_close = prices.first().and_then(|p| p.close_price);

    let listing_gain = match (issue_price, listing_close) {
        (Some(ip), Some(lc)) => format::pct_change(ip, lc).map(format::signed_pct),
        _ => None,
    };
    let current_gain = match (issue_price, last_close) {
        (Some(ip), Some(lc)) => format::pct_change(ip, lc).map(format::signed_pct),
        _ => None,
    };

    let header = h_flex()
        .p(px(S3))
        .gap(px(S3))
        .child(
            metric("Issue price", issue_price.map(format::rupees).unwrap_or_else(|| "—".to_string()), &palette),
        )
        .child(
            metric("Listing close", listing_close.map(format::rupees).unwrap_or_else(|| "—".to_string()), &palette),
        )
        .child(
            metric(
                "Listing gain",
                listing_gain.unwrap_or_else(|| "—".to_string()),
                &palette,
            ),
        )
        .child(
            metric(
                "Current close",
                last_close.map(format::rupees).unwrap_or_else(|| "—".to_string()),
                &palette,
            ),
        )
        .child(
            metric(
                "Current gain",
                current_gain.unwrap_or_else(|| "—".to_string()),
                &palette,
            ),
        );

    let chart = price_chart(prices, issue_price, &palette);

    // OHLCV table.
    let mut ohlcv = div().flex().flex_col().px(px(S3)).pb(px(S3));
    let header_row = h_flex()
        .h(px(22.))
        .gap(px(S2))
        .child(cell("Date", 80., palette.text_disabled, &palette))
        .child(cell("Open", 70., palette.text_disabled, &palette))
        .child(cell("High", 70., palette.text_disabled, &palette))
        .child(cell("Low", 70., palette.text_disabled, &palette))
        .child(cell("Close", 70., palette.text_disabled, &palette))
        .child(cell("Volume", 100., palette.text_disabled, &palette));
    ohlcv = ohlcv.child(header_row);

    for (i, p) in prices.iter().rev().take(30).enumerate() {
        let bg = if i % 2 == 1 { palette.element } else { palette.panel };
        let close_color = p
            .close_price
            .zip(issue_price)
            .map(|(c, ip)| if c >= ip { palette.up } else { palette.down })
            .unwrap_or(palette.text);
        let row = h_flex()
            .h(px(ROW_H))
            .gap(px(S2))
            .bg(bg)
            .child(cell(&format::date(p.trade_date), 80., palette.text_muted, &palette))
            .child(cell(&fmt_opt(p.open_price), 70., palette.text_muted, &palette))
            .child(cell(&fmt_opt(p.high_price), 70., palette.text_muted, &palette))
            .child(cell(&fmt_opt(p.low_price), 70., palette.text_muted, &palette))
            .child(cell(&fmt_opt(p.close_price), 70., close_color, &palette))
            .child(cell(&p.volume.map(format::shares).unwrap_or_else(|| "—".to_string()), 100., palette.text_muted, &palette));
        ohlcv = ohlcv.child(row);
    }

    div().child(
        v_flex()
            .id(gpui::ElementId::Name("dossier-perf".into()))
            .flex_1()
            .overflow_y_scroll()
            .child(header)
            .child(chart)
            .child(section_label_at("OHLCV — LAST 30 SESSIONS", &palette))
            .child(ohlcv),
    )
}

fn metric(label: &str, value: String, palette: &crate::theme::Palette) -> gpui::Div {
    v_flex()
        .gap(px(2.))
        .child(div().text_size(px(10.)).text_color(palette.text_disabled).child(label.to_string()))
        .child(div().text_size(px(FONT_UI)).font_weight(gpui::FontWeight::MEDIUM).text_color(palette.text).child(value))
}

fn section_label_at(label: &str, palette: &crate::theme::Palette) -> gpui::Div {
    div()
        .px(px(S3))
        .pt(px(S2))
        .text_size(px(10.))
        .text_color(palette.text_disabled)
        .child(label.to_string())
}

fn fmt_opt(d: Option<rust_decimal::Decimal>) -> String {
    d.map(|d| d.round_dp(2).to_string()).unwrap_or_else(|| "—".to_string())
}

fn cell(text: &str, width: f32, color: gpui::Hsla, _palette: &crate::theme::Palette) -> gpui::Div {
    div()
        .w(px(width))
        .truncate()
        .text_size(px(10.))
        .text_color(color)
        .child(text.to_string())
}

/// Line chart of daily closes with a dashed reference line at the issue
/// price (ADR-0004: Chittorgarh/Moneycontrol pattern).
fn price_chart(
    prices: &[mosaic_core::PricePoint],
    issue_price: Option<rust_decimal::Decimal>,
    palette: &crate::theme::Palette,
) -> gpui::Div {
    let closes: Vec<f64> = prices
        .iter()
        .filter_map(|p| p.close_price.and_then(|c| c.to_f64()))
        .collect();
    let reference = issue_price.and_then(|p| p.to_f64());

    let mut min = closes.iter().cloned().fold(f64::INFINITY, f64::min);
    let mut max = closes.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    if let Some(r) = reference {
        min = min.min(r);
        max = max.max(r);
    }
    if !min.is_finite() || !max.is_finite() || (max - min).abs() < f64::EPSILON {
        max = min + 1.0;
    }

    let up = palette.up;
    let down = palette.down;
    let reference_color = reference
        .map(|r| if closes.last().copied().unwrap_or(r) >= r { up } else { down })
        .unwrap_or(palette.text_muted);
    let border = palette.border_variant;
    let text_muted = palette.text_muted;

    let chart = canvas(
        move |bounds, _window, _cx| {
                let _ = bounds;
                (closes.clone(), reference, reference_color, min, max, border, text_muted)
            },
        move |bounds, (closes, reference, reference_color, min, max, border, text_muted), window, _cx| {
            let origin = bounds.origin;
            let size = bounds.size;
            let w = size.width.to_f64();
            let h = size.height.to_f64();
            let pad_l = 8.0;
            let pad_r = 8.0;
            let pad_t = 8.0;
            let pad_b = 8.0;
            let plot_w = (w - pad_l - pad_r).max(1.0);
            let plot_h = (h - pad_t - pad_b).max(1.0);

            let x = |i: usize| pad_l + (i as f64 / (closes.len().saturating_sub(1) as f64).max(1.0)) * plot_w;
            let y = |v: f64| pad_t + (1.0 - (v - min) / (max - min).max(1e-9)) * plot_h;

            // Reference line (dashed).
            if let Some(r) = reference {
                let mut path = gpui::Path::new(gpui::point(origin.x, origin.y));
                let mut d = 0.0;
                let yref = y(r);
                while d < plot_w {
                    path.move_to(gpui::Point {
                        x: origin.x + px(pad_l as f32 + d as f32),
                        y: origin.y + px(yref as f32),
                    });
                    path.line_to(gpui::Point {
                        x: origin.x + px((pad_l + d + 5.0) as f32),
                        y: origin.y + px(yref as f32),
                    });
                    d += 10.0;
                }
                window.paint_path(path, reference_color);
            }

            // Close-price line.
            let mut path = gpui::Path::new(gpui::point(origin.x, origin.y));
            for (i, v) in closes.iter().enumerate() {
                let p = gpui::Point {
                    x: origin.x + px(x(i) as f32),
                    y: origin.y + px(y(*v) as f32),
                };
                if i == 0 {
                    path.move_to(p);
                } else {
                    path.line_to(p);
                }
            }
            window.paint_path(path, text_muted);

            // Baseline.
            let mut baseline = gpui::Path::new(gpui::point(origin.x, origin.y));
            baseline.move_to(gpui::Point {
                x: origin.x + px(pad_l as f32),
                y: origin.y + px((pad_t + plot_h) as f32),
            });
            baseline.line_to(gpui::Point {
                x: origin.x + px((pad_l + plot_w) as f32),
                y: origin.y + px((pad_t + plot_h) as f32),
            });
            window.paint_path(baseline, border);
        },
    );

    div()
        .mx(px(S3))
        .h(px(220.))
        .bg(palette.editor)
        .rounded(px(RADIUS_SM))
        .border_1()
        .border_color(palette.border_variant)
        .child(chart)
}

// ---------------------------------------------------------------------------
// Docs
// ---------------------------------------------------------------------------

fn docs_view(app: &MosaicApp, ipo: &Ipo) -> gpui::Div {
    let _ = app;
    let palette = crate::theme::palette();

    let mut col = v_flex().flex_1().p(px(S3)).gap(px(S1));

    let mut links: Vec<(String, Option<String>)> = Vec::new();
    links.push(("Red Herring Prospectus (RHP)".to_string(), ipo.rhp_url.clone()));
    links.push(("Draft Red Herring Prospectus (DRHP)".to_string(), ipo.drhp_url.clone()));
    if let Some(url) = &ipo.detail_url {
        links.push(("Chittorgarh detail page".to_string(), Some(url.clone())));
    }

    for (label, url) in links {
        let row: gpui::Div = match url {
            Some(url) => div().child(
                div()
                .id(gpui::ElementId::Name(label.clone().into()))
                .h(px(28.))
                .px(px(S2))
                .rounded(px(RADIUS_SM))
                .flex()
                .flex_row()
                .items_center()
                .justify_between()
                .cursor_pointer()
                .hover(|this| this.bg(palette.hover))
                .on_click(move |_, _, cx: &mut gpui::App| {
                    if let Err(e) = open::that_detached(&url) {
                        log::error!("open {url}: {e}");
                    }
                    let _ = cx;
                })
                .child(div().text_size(px(FONT_UI)).text_color(palette.text_accent).child(label))
                .child(div().text_size(px(10.)).text_color(palette.text_disabled).child("↗")),
            ),
            None => div()
                .h(px(28.))
                .px(px(S2))
                .rounded(px(RADIUS_SM))
                .flex()
                .flex_row()
                .items_center()
                .text_size(px(FONT_SM))
                .text_color(palette.text_disabled)
                .child(label),
        };
        col = col.child(row);
    }

    col.child(
        div()
            .pt(px(S2))
            .text_size(px(10.))
            .text_color(palette.text_disabled)
            .child("Source documents open in your browser. All figures in Mosaic trace to NSE/Chittorgarh with ingested_at timestamps."),
    )
}
